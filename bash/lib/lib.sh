#!/usr/bin/env bash

# Copyright 2023 The Artifact Executor Authors. All rights reserved.
# Use of this source code is governed by a Apache-style license that can be
# found in the LICENSE file.

#
# General-purpose library functions.
#

set -eo pipefail

# Support "gnu date" from Mac HomeBrew configurations.
if [[ "$(which gdate)" != "" ]]; then
  timestamp () {
    local -n timestamp_timestamp="$1"
    timestamp_timestamp=$(gdate '+%s.%N')
  }
else
  timestamp () {
    local -n timestamp_timestamp="$1"
    timestamp_timestamp=$(date '+%s.%N')
  }
fi

declare -A tmp_dirs
if [[ "${tmp_dirs[$BASHPID]}" == "" ]]; then
  tmp_dirs[$BASHPID]=$(realpath $(mktemp -d))
fi

mk_temp_file () {
  local -n mtf_path="$1"
  local mtf_timestamp=""
  timestamp mtf_timestamp
  mtf_path="${tmp_dirs[$BASHPID]}/f${mtf_timestamp}-${RANDOM}"
  touch "${mtf_path}"
}

mk_temp_dir () {
  local -n mtd_path="$1"
  local mtd_timestamp=""
  timestamp mtd_timestamp
  mtd_path="${tmp_dirs[$BASHPID]}/d${mtd_timestamp}-${RANDOM}"
  mkdir "${mtd_path}"
}

clean_up_temp_files_and_dirs () {
  rm -rf "${tmp_dirs[$BASHPID]}"
}

# Convert associative array $1 into an array $2 by copying "<key>=<value>".
map_to_array () {
  # mta stands for map_to_array.
  local -n mta_map="$1"
  local -n mta_array="$2"
  for key in "${!mta_map[@]}"; do
    mta_array+=("${key}=${mta_map[${key}]}")
  done
}

# Append each element in array $1 to file $2.
append_array_to_file () {
  # aatf stands for append_array_to_file.
  local -n aatf_env_array="$1"
  local -n aatf_output_file="$2"
  for env_binding in "${aatf_env_array[@]}"; do
    printf '%s\n' "${env_binding}" >> "${aatf_output_file}"
  done
}

# Compute the sha256 hash of the file $1 and store it in $2.
sha256_file () {
  # sf stands for sha256_file.
  local -n sf_input_file="$1"
  local -n sf_sha256_hash="$2"
  sf_sha256_hash=$(sha256sum "${sf_input_file}" -b | cut -d ' ' -f1)
}

# Move file $1 to $2 and update variable $1 to $2.
mv_file () {
  # mf stands for mv_file.
  local -n mf_src="$1"
  local -n mf_dst="$2"
  mv "${mf_src}" "${mf_dst}"
  chmod u+w "${mf_dst}"
  mf_src="${mf_dst}"
}

summarize_action () {
  # sa stands for summarize_action.
  local -n sa_program="$1"
  local -n sa_args_array="$2"
  local -n sa_out="$3"

  if (( ${#sa_args_array[@]} < 5 )); then
    sa_out="${sa_program} ${sa_args_array[@]}"
  else
    sa_out+="${sa_program} ${sa_args_array[0]} "
    if [[ "${sa_args_array[0]:0:1}" == "-" && "${sa_args_array[1]:0:1}" != "-" ]]; then
      sa_out+="${sa_args_array[1]} "
    fi
    sa_out+="... "
    if [[ "${sa_args_array[-2]:0:1}" == "-" && "${sa_args_array[-1]:0:1}" != "-" ]]; then
      sa_out+="${sa_args_array[-2]} "
    fi
    sa_out+="${sa_args_array[-1]}"
  fi
}

is_file_cached () {
  local -n cf_cache_dir="$1"
  local -n cf_source_path="$2"
  local -n cf_result="$3"

  local cache_path_dir="${cf_cache_dir}/path"
  local cache_sha256_dir="${cf_cache_dir}/sha256"
  if [[ -f "${cf_source_path}" && -f "${cache_path_dir}${cf_source_path}" ]]; then
    if [[ "${cache_path_dir}${cf_source_path}" -nt "${cf_source_path}" ]]; then
      cf_result="true"
    else
      cf_result="false"
    fi
  else
    cf_result="false"
  fi
}

# Cache in cach dir $1 the file stored at $2, outputting the file sha256 hash to $3 and its size to
# $4. Optionally, the "real file path" can be passed via $5; this parameter is used in contexts
# where, for example, code is caching an output file from within a sandbox directory, but the file
# has not yet been copied to its unsandboxed destination.
cache_file () {
  # cf stands for cache_file.
  local -n cf_cache_dir="$1"
  local -n cf_source_path="$2"
  local -n cf_file_hash="$3"
  local -n cf_file_size="$4"

  set +e
  local -n cf_file_path_ref="$5" > /dev/null 2>&1
  set -eo pipefail
  if [[ "${cf_file_path_ref}" == "" ]]; then
    local cf_file_path="${cf_source_path}"
  else
    local cf_file_path="${cf_file_path_ref}"
  fi

  local cache_path_dir="${cf_cache_dir}/path"
  local cache_sha256_dir="${cf_cache_dir}/sha256"
  if [[ -f "${cache_path_dir}${cf_file_path}" ]]; then
    if [[ "${cache_path_dir}${cf_file_path}" -nt "${cf_source_path}" ]]; then
      IFS="|" read -r read_hash read_size < "${cache_path_dir}${cf_file_path}"
      if [[ ! -f "${cache_sha256_dir}/${read_hash}" ]]; then
        cp "${cf_source_path}" "${cache_sha256_dir}/${read_hash}"
        chmod u+w "${cache_sha256_dir}/${read_hash}"
      fi

      cf_file_hash="${read_hash}"
      cf_file_size="${read_size}"
      return
    fi
  fi

  sha256_file cf_source_path cf_file_hash
  if [[ ! -f "${cache_sha256_dir}/${cf_file_hash}" ]]; then
    cp "${cf_source_path}" "${cache_sha256_dir}/${cf_file_hash}"
    chmod u+w "${cache_sha256_dir}/${cf_file_hash}"
  fi
  cf_file_size=$(wc -c < "${cf_source_path}")

  local cache_path_subdir=$(dirname "${cache_path_dir}${cf_file_path}")
  if [[ ! -d "${cache_path_subdir}" ]]; then
    mkdir -p "${cache_path_subdir}"
  fi
  printf '%s|%s\n' "${cf_file_hash}" "${cf_file_size}" > "${cache_path_dir}${cf_file_path}"
}

# cache_file_bg () {
#   # cfb stands for cache_file_bg.
#   local -n cfb_cache_dir="$1"
#   local -n cfb_source_path="$2"
#   local -n cfb_file_hash="$3"
#   local -n cfb_file_size="$4"
#   local -n cfb_pid="$4"

#   set +e
#   local -n cfb_file_path_ref="$5" > /dev/null 2>&1
#   set -eo pipefail

#   if [[ "${cfb_file_path_ref}" == "" ]]; then
#     (

#     ) &
#   else
#   fi
# }
