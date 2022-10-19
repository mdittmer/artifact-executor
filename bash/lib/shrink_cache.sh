#!/usr/bin/env bash

#
# Interactive cache eviction that prompts user to discard particular executions' cached artifacts.
#

set -eo pipefail

source "$(dirname ${BASH_SOURCE[0]})/lib.sh"

source "$(dirname ${BASH_SOURCE[0]})/log.sh"
init_logging default_log_level_config

pre_prompt () {
  exec 3<&0
}

post_prompt () {
  exec 3<&-
}

with_prompt () {
  pre_prompt
  local fn="$1"
  shift
  "${fn}" "$@"
  post_prompt
}

prompt () {
  local p_prompt="$1"
  local -n p_opts_map="$2"
  local -n p_result="$3"
  # local read_ch=""
  local done="false"

  while [[ "${done}" == "false" ]]; do
    >&2 printf "${p_prompt}\n[ "
    for ch in "${!p_opts_map[@]}"; do
      >&2 printf "(${ch})${p_opts_map[${ch}]} "
    done
    >&2 printf "]?  "

    read -n1 read_ch <&3
    >&2 printf "\n"

    for ch in "${!p_opts_map[@]}"; do
      if [[ "${ch}" == "${read_ch}" ]]; then
        p_result="${read_ch}"
        done="true"
        break
      fi
    done
  done
}

shrink_cache () {
  with_prompt "_shrink_cache" "$@"
}

_shrink_cache () {
  set +e
  local -n sc_cache_dir_ref="$1" > /dev/null 2>&1
  set -eo pipefail
  if [[ "${sc_cache_dir_ref}" == "" ]]; then
    if [[ "${ARTIFACT_EXECUTOR_CACHE}" == "" ]]; then
      log_error "Artifact executor cache directory unset; pass it as a parameter to shrink_cache or set the environment variable ARTIFACT_EXECUTOR_CACHE"
      exit 1
    else
      local sc_cache_dir="${ARTIFACT_EXECUTOR_CACHE}"
    fi
  else
    local sc_cache_dir="${sc_cache_dir_ref}"
  fi

  local sha256_dir="${sc_cache_dir}/sha256"
  local path_dir="${sc_cache_dir}/path"
  local wepai_dir="${sc_cache_dir}/wd_env_pogram_args_inputs_sha256"

  if [[ ! -d "${sha256_dir}" ]]; then
    log_error "Artifact executor missing cache subdir \"${sha256_dir}\""
    exit 1
  fi
  if [[ ! -d "${wepai_dir}" ]]; then
    log_error "Artifact executor missing cache subdir \"${wepai_dir}\""
    exit 1
  fi

  local -A unused_hash_cache
  while IFS="" read -r cached_file || [ -n "$cached_file" ]; do
    if [[ "${cached_file}" == "" ]]; then
      continue
    fi
    unused_hash_cache[${cached_file}]=0
  done < <(ls -t "${sha256_dir}")

  local -A unused_path_cache
  while IFS="" read -r cached_file || [ -n "$cached_file" ]; do
    if [[ "${cached_file}" == "" ]]; then
      continue
    fi
    unused_path_cache[${cached_file:${#path_dir}}]=0
  done < <(find "${path_dir}" -type f)

  local -A used_hash_cache
  while IFS="" read -r wepai_file || [ -n "$wepai_file" ]; do
    if [[ "${wepai_file}" == "" ]]; then
      continue
    fi
    IFS='|' read -r wdfh efh ph afh imfh tomfh < "${wepai_dir}/${wepai_file}"
    local -a wepai_hashes_array=(
      "${wdfh}"
      "${efh}"
      "${ph}"
      "${afh}"
      "${imfh}"
      "${tomfh}"
    )
    for hash in "${wepai_hashes_array[@]}"; do
      if [[ "${unused_hash_cache[${hash}]}" != "" ]]; then
        unset unused_hash_cache[${hash}]
        used_hash_cache[${hash}]=1
      else
        ((used_hash_cache[${hash}]++))
      fi
    done
  done < <(ls -t "${wepai_dir}")

  while IFS="" read -r wepai_file || [ -n "$wepai_file" ]; do
    if [[ "${wepai_file}" == "" ]]; then
      continue
    fi
    IFS='|' read -r wdfh efh ph afh imfh tomfh < "${wepai_dir}/${wepai_file}"
    local -A wepai_hashes_map=(
      ["working directory"]="${wdfh}"
      ["environment file"]="${efh}"
      ["program"]="${ph}"
      ["arguments"]="${afh}"
      ["inputs manifest"]="${imfh}"
      ["outputs manifest"]="${tomfh}"
    )
    local -i min_cached_bytes=0
    local -i max_cached_bytes=0
    for description in "${!wepai_hashes_map[@]}"; do
      hash=${wepai_hashes_map["${description}"]}
      if [[ ! -f "${sha256_dir}/${hash}" ]]; then
        log_warning "Action stored in ${wepai_file} is missing ${description} file, expected at ${sha256_dir}/${hash}"
        continue
      fi
      file_size=$(wc -c < "${sha256_dir}/${hash}")
      ((max_cached_bytes+="${file_size}"))
      if [[ "${used_hash_cache[${hash}]}" == "1" ]]; then
        ((min_cached_bytes+="${file_size}"))
      fi
    done

    if [[ "${unused_hash_cache[${wepai_file}]}" != "" ]]; then
      unset unused_hash_cache[${wepai_file}]
      used_hash_cache[${wepai_file}]=1
    else
      ((used_hash_cache[${wepai_file}]++))
    fi

    local -A used_path_cache
    local program="<unknown program>"
    if [[ ! -f "${sha256_dir}/${imfh}" ]]; then
      log_warning "Action cached at ${wepai_dir}/${wepai_file} refers to uncached input manifest ${sha256_dir}/${imfh}"
    else
      while IFS="|" read -r path hash size; do
        if [[ "${hash}" == "${ph}" ]]; then
          program="${path}"
        fi
        if [[ "${unused_hash_cache[${hash}]}" != "" ]]; then
          unset unused_hash_cache[${hash}]
          used_hash_cache[${hash}]=1
        else
          ((used_hash_cache[${hash}]++))
        fi
        if [[ "${unused_path_cache[${path}]}" != "" ]]; then
          unset unused_path_cache[${path}]
          used_path_cache[${path}]=1
        else
          ((used_path_cache[${path}]++))
        fi
        local path_cache_size=$(wc -c < "${path_dir}${path}")
        ((max_cached_bytes+=${size}+${path_cache_size}))
        if [[ "${used_hash_cache[${hash}]}" == "1" ]]; then
          ((min_cached_bytes+="${size}"))
        fi
        if [[ "${used_path_cache[${path}]}" == "1" ]]; then
          ((min_cached_bytes+="${path_cache_size}"))
        fi
      done < "${sha256_dir}/${imfh}"
    fi

    if [[ ! -f "${sha256_dir}/${tomfh}" ]]; then
      log_warning "Action cached at ${wepai_dir}/${wepai_file} refers to uncached output manifest ${sha256_dir}/${tomfh}"
    else
      while IFS="|" read -r path hash size; do
        if [[ "${unused_hash_cache[${hash}]}" != "" ]]; then
          unset unused_hash_cache[${hash}]
          used_hash_cache[${hash}]=1
        else
          ((used_hash_cache[${hash}]++))
        fi
        if [[ "${unused_path_cache[${path}]}" != "" ]]; then
          unset unused_path_cache[${path}]
          used_path_cache[${path}]=1
        else
          ((used_path_cache[${path}]++))
        fi
        local path_cache_size=$(wc -c < "${path_dir}${path}")
        ((max_cached_bytes+=${size}+${path_cache_size}))
        if [[ "${used_hash_cache[${hash}]}" == "1" ]]; then
          ((min_cached_bytes+="${size}"))
        fi
        if [[ "${used_path_cache[${path}]}" == "1" ]]; then
          ((min_cached_bytes+="${path_cache_size}"))
        fi
      done < "${sha256_dir}/${tomfh}"
    fi

    mapfile -t args_array < "${sha256_dir}/${afh}"
    local modified_date=$(stat -c "%y" "${wepai_dir}/${wepai_file}")
    local cached_bytes=""
    if [[ "${min_cached_bytes}" == "${max_cached_bytes}" ]]; then
      cached_bytes="${max_cached_bytes} bytes"
    else
      cached_bytes="${min_cached_bytes} - ${max_cached_bytes} bytes"
    fi
    local action_summary=""
    summarize_action program args_array action_summary

    local -A opts=(
      [r]="emove"
      [s]="kip"
      [q]="uit"
    )
    local opt=""

    prompt "    ${action_summary}\n    ${cached_bytes}\n    ${modified_date}" opts opt

    if [[ "${opt}" == "r" ]]; then
      if [[ -f "${sha256_dir}/${imfh}" ]]; then
        while IFS="|" read -r path hash size; do
          ((used_hash_cache[${hash}]--))
          if [[ "${used_hash_cache[${hash}]}" == "0" ]]; then
            unset used_hash_cache[${hash}]
            unused_hash_cache[${hash}]=0
          fi
          ((used_path_cache[${path}]--))
          if [[ "${used_path_cache[${path}]}" == "0" ]]; then
            unset used_path_cache[${path}]
            unused_path_cache[${path}]=0
          fi
        done < "${sha256_dir}/${imfh}"
      fi
      if [[ -f "${sha256_dir}/${tomfh}" ]]; then
        while IFS="|" read -r path hash size; do
          ((used_hash_cache[${hash}]--))
          if [[ "${used_hash_cache[${hash}]}" == "0" ]]; then
            unset used_hash_cache[${hash}]
            unused_hash_cache[${hash}]=0
          fi
          ((used_path_cache[${path}]--))
          if [[ "${used_path_cache[${path}]}" == "0" ]]; then
            unset used_path_cache[${path}]
            unused_path_cache[${path}]=0
          fi
        done < "${sha256_dir}/${tomfh}"
      fi
      for hash in "${wepai_hashes_map[@]}"; do
        ((used_hash_cache[${hash}]--))
        if [[ "${used_hash_cache[${hash}]}" == "0" ]]; then
          unset used_hash_cache[${hash}]
          unused_hash_cache[${hash}]=0
        fi
      done
      ((used_hash_cache[${wepai_file}]--))
      if [[ "${used_hash_cache[${wepai_file}]}" == "0" ]]; then
        unset used_hash_cache[${wepai_file}]
        unused_hash_cache[${wepai_file}]=0
      fi
      rm "${wepai_dir}/${wepai_file}"
    elif [[ "${opt}" == "q" ]]; then
      break
    fi
  done < <(ls -t -r "${wepai_dir}")

  for hash in "${!unused_hash_cache[@]}"; do
    rm "${sha256_dir}/${hash}"
  done
  for path in "${!unused_path_cache[@]}"; do
    rm "${path_dir}${path}"
  done
  find "${path_dir}" -type d -empty -delete
}

shrink_cache
