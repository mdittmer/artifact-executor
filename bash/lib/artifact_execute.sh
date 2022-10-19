#!/usr/bin/env bash

#
# Core implementation of executing a binary with Artifact Executor.
#

set -eo pipefail

declare LIB_DIR=$(dirname ${BASH_SOURCE[0]})
declare AE="${LIB_DIR}/../bin/artifact-executor"

source "${LIB_DIR}/env.sh"
source "${LIB_DIR}/lib.sh"
source "${LIB_DIR}/trace.sh"

source "${LIB_DIR}/log.sh"
init_logging default_log_level_config

# Quote for literal use in `grep -E`. Shamelessly stolen from
# https://stackoverflow.com/questions/11856054/is-there-an-easy-way-to-pass-a-raw-string-to-grep.
egrep_quote () {
    sed 's/[][\.|$(){}?+*^]/\\&/g' <<< "$*"
}

esed_quote () {
  sed 's/\//\\\//g' < <(egrep_quote $*)
}

# Rebase path(s) in an environment variable value of the form <path1>:<path2>:...:<pathN>. The path
# to rebase over is given as a "quoted for `sed -E`" string $1; the environment variable value is
# $2; the output variable for the new value is $3.
rebase_paths_environment_variable_value_with_esed_quoted_path () {
  # rpevvwep stands for rebase_paths_environment_variable_value_with_esed_quoted_path.
  local -n rpevvwep_path="$1"
  local -n rpevvwep_value="$2"
  local -n rpevvwep_output_value="$3"
  rpevvwep_output_value=$(sed -E -e "s/^\//${rpevvwep_path}\//g" -e "s/:\//:${rpevvwep_path}\//g" <<< "${rpevvwep_value}")
}

# Rebase environment variable values assuming that rebase-able absolute paths appear:
#
# 1. Where the first character of the environment variable value is '/', or
# 2. When the string ":/" appears in the environment variable value.
#
# For example, rebasing the environment variable binding `PATH=/usr/bin:./bin:/usr/sbin` over the
# path "/sandbox" yields `PATH=/sandbox/usr/bin:./bin:/sandbox/usr/sbin`.
#
# This is the default strategy for producing a "sandbox-appropriate" environment map based on a
# sandbox directory and an "untransformed" environment map.
#
# The directory to rebase over is $1; the input environment variable map is $2; the output
# environment variable map is $3.
rebase_paths_in_environment () {
  # rpie stands for rebase_paths_in_environment.
  local -n rpie_sandbox_dir="$1"
  local -n rpie_input_env="$2"
  local -n rpie_output_env="$3"
  local quoted_sandbox_dir=$(esed_quote "${rpie_sandbox_dir}")
  for key in "${!rpie_input_env[@]}"; do
    local value="${rpie_input_env[${key}]}"
    local new_value=""
    rebase_paths_environment_variable_value_with_esed_quoted_path quoted_sandbox_dir value new_value
    rpie_output_env["${key}"]="${new_value}"
  done
}

# Find the longest common path segments prefix (not containing trailing slash) from $1 and store it
# in $2.
#
# Shamelessly modified from
# https://stackoverflow.com/questions/12340846/bash-shell-script-to-find-the-closest-parent-directory-of-several-files
longest_common_absolute_path_prefix () {
  # lcapp stands for longest_common_absolute_path_prefix.
  declare -n lcapp_paths="$1"
  declare -n lcapp_prefix="$2"
  declare -a parts
  declare -i i=0

  for path in "${lcapp_paths[@]}"; do
    if [[ "${path:0:1}" != "/" ]]; then
      log_error "Non-absolute path \"${path}\" passed to longest_common_absolute_path_prefix"
      log "All paths: "$(printf "\"%s\" " "${lcapp_paths[@]}")
      exit 1
    fi
  done

  name="${lcapp_paths[0]}"
  while x=$(dirname "$name"); [ "$x" != "/" ]; do
    parts[$i]="$x"
    i=$(($i + 1))
    name="$x"
  done

  for prefix in "${parts[@]}"; do
    for name in "${lcapp_paths[@]}"; do
      if [[ "${name#$prefix/}" == "${name}" ]]; then
        continue 2
      fi
    done
    lcapp_prefix="${prefix}"
    break
  done

  log_error "Failed to determine longest path prefix among paths"
  log "All paths: "$(printf "\"%s\" " "${lcapp_paths[@]}")
  exit 1
}

# Drop path-prefix (not containing trailing slash) $1 from paths in array $2 and store the result
# in new array $3.
paths_without_prefixes () {
  # pwp stands for paths_without_prefixes.
  local -n pwp_prefix="$1"
  local -n pwp_paths="$2"
  local -n pwp_result_paths="$3"
  for path in "${pwp_paths[@]}"; do
    pwp_result_paths+=("${path#${pwp_prefix}/}")
  done
}

# Create in sandbox directory $1, the directory at $2, outputting the new path at $3.
create_sandboxed_dir () {
  # csd stands for create_sandboxed_dir.
  local -n csd_sandbox="$1"
  local -n csd_dir_to_sandbox="$2"
  local -n csd_sandboxed_dir="$3"
  local real_dir=$(realpath -Ls "${csd_dir_to_sandbox}")
  local sandboxed_dir="${csd_sandbox}${real_dir}"
  mkdir -p "${sandboxed_dir}"
  csd_sandboxed_dir="${sandboxed_dir}"
}

# Copy into sandbox directory $1, the file at $2, outputting the new path at $3.
copy_to_sandbox () {
  # cts stands for copy_to_sandbox.
  local -n cts_sandbox="$1"
  local -n cts_file="$2"
  local -n cts_dst_file="$3"
  local real_file=$(realpath -Ls "${cts_file}")
  local file_name=$(basename "${real_file}")
  local file_dir="${cts_sandbox}$(dirname ${real_file})"
  mkdir -p "${file_dir}"
  local destination="${file_dir}/${file_name}"
  cp "${real_file}" "${destination}"
  chmod u+w "${destination}"
  cts_dst_file="${destination}"
}

# Rebase absolute paths in array $2 with prefix root directory $1, outputting new paths to array $3.
rebase_absolute_paths () {
  # rap stands for rebase_absolute_paths.
  local -n rap_root="$1"
  local -n rap_paths="$2"
  local -n rap_rebased_paths="$3"
  rap_rebased_paths=()
  for path in "${rap_paths[@]}"; do
    if [[ "${path:0:1}" == "/" ]]; then
      rap_rebased_paths+=("${rap_root}${path}")
    else
      rap_rebased_paths+=("${path}")
    fi
  done
}

check_hermetic_files () {
  # chi stands for check_hermetic_inputs.
  local -n chi_sandbox_dir="$1"
  local -n chi_files_file="$2"
  local non_hermetic_inputs_file=""
  mk_temp_file non_hermetic_inputs_file
  set +e
  grep -v -E "^$(egrep_quote ${chi_sandbox_dir})" "${chi_files_file}" > "${non_hermetic_inputs_file}"
  local grep_status="$?"
  set -eo pipefail
  case "${grep_status}" in
    0)
      local found_non_hermetic="false"
      while IFS="" read -r path || [ -n "${path}" ]; do
        # Accessing `/proc/...` is likely to be ephemeral files related to the running process.
        if [[ "${path}" =~ ^/proc/ ]]; then
          continue
        fi

        local sandbox_diff_status=""
        if [[ ! -f "${path}" ]]; then
          sandbox_diff_status=1
        elif [[ ! -f "${chi_sandbox_dir}${path}" ]]; then
          sandbox_diff_status=1
        else
          set +e
          diff "${path}" "${chi_sandbox_dir}${path}" > /dev/null 2>&1
          sandbox_diff_status="$?"
          set -eo pipefail
        fi

        case "${sandbox_diff_status}" in
          0)
            log_warning "Non-hermetic read of ${path}, but un/sandboxed file contents appear to be the same"
            ;;
          1)
            log_error "Non-hermetic read of ${path} with different un/sandboxed file contents"
            found_non_hermetic="true"
            ;;
          *)
            log_error "Unexpected status code from diff while checking hermetic files: ${sandbox_diff_status}"
            exit 1
            ;;
        esac
      done < "${non_hermetic_inputs_file}"
      if [[ "${found_non_hermetic}" != "false" ]]; then
        exit 1
      fi
      ;;
    1)
      ;;
    *)
      log_error "Unexpected status code from grep while checking hermetic files: ${grep_status}"
      exit 1
      ;;
  esac
  # rm "${non_hermetic_inputs_file}"
}

check_traced_inputs_against_declared () {
  # ctiad stands for check_traced_inputs_against_declared.
  local -n ctiad_sandbox_dir="$1"
  local quoted_sandbox_dir=$(esed_quote "${ctiad_sandbox_dir}")

  local -n ctiad_expected_inputs_file="$2"
  local -n ctiad_actual_inputs_file="$3"
  mapfile -t ctiad_expected_inputs < "${ctiad_expected_inputs_file}"
  mapfile -t ctiad_actual_inputs < "${ctiad_actual_inputs_file}"

  local expected_inputs_file=""
  mk_temp_file expected_inputs_file
  (
    for input in ${ctiad_expected_inputs[@]}; do printf '%s\n' "${input}"; done
  ) | sort > "${expected_inputs_file}"

  local actual_inputs_file=""
  mk_temp_file actual_inputs_file
  (
    for input in ${ctiad_actual_inputs[@]}; do
      # Accessing `/proc/...` is likely to be ephemeral files related to the running process.
      if [[ ! "${input}" =~ ^/proc/ ]]; then
        # Drop sandbox directory prefix from actual hermetic inputs.
        printf '%s\n' "${input}" | sed "s/^${quoted_sandbox_dir}//"
      fi
    done
  ) | sort > "${actual_inputs_file}"

  local diff_file=""
  mk_temp_file diff_file
  set +e
  diff "${expected_inputs_file}" "${actual_inputs_file}" > "${diff_file}"
  local diff_status="$?"
  set -eo pipefail
  case  "${diff_status}" in
    0)
      ;;
    1)
      local not_expected_file=""
      mk_temp_file not_expected_file
      set +e
      grep '^[>]' "${diff_file}" > "${not_expected_file}"
      local grep_status="$?"
      set -eo pipefail
      case "${grep_status}" in
        0)
          log_error "Unexpected input files detected"
          log_file "${not_expected_file}"
          log "Expected inputs listed in ${expected_inputs_file}"
          log "Actual inputs listed in ${actual_inputs_file}"
          exit 1
          ;;
        1)
          ;;
        *)
          log_error "Unexpected status code from grep while checking inputs: ${grep_status}"
          exit 1
          ;;
      esac
      # TODO: Emit warning if not all inputs touched.
     # rm "${not_expected_file}"
      ;;
    *)
      log_error "Unexpected status code from diff: ${diff_status}"
      exit 1
      ;;
  esac
# rm "${expected_inputs_file}"
# rm "${actual_inputs_file}"
# rm "${diff_file}"
}

check_traced_outputs_against_declared () {
  # ctoad stands for check_traced_outputs_against_declared.
  local -n ctoad_sandbox_dir="$1"
  local quoted_sandbox_dir=$(esed_quote "${ctoad_sandbox_dir}")

  local -n ctoad_expected_outputs_file="$2"
  local -n ctoad_actual_outputs_file="$3"
  mapfile -t ctoad_expected_outputs < "${ctoad_expected_outputs_file}"
  mapfile -t ctoad_actual_outputs < "${ctoad_actual_outputs_file}"

  local expected_outputs_file=""
  mk_temp_file expected_outputs_file
  (
    for input in ${ctoad_expected_outputs[@]}; do printf '%s\n' "${input}"; done
  ) | sort > "${expected_outputs_file}"

  local actual_outputs_file=""
  mk_temp_file actual_outputs_file
  (
    for input in ${ctoad_actual_outputs[@]}; do
      # Drop sandbox directory prefix from actual hermetic outputs.
      printf '%s\n' "${input}" | sed "s/^${quoted_sandbox_dir}//"
    done
  ) | sort > "${actual_outputs_file}"

  local diff_file=""
  mk_temp_file diff_file
  set +e
  diff "${expected_outputs_file}" "${actual_outputs_file}" > "${diff_file}"
  local diff_status="$?"
  set -eo pipefail
  case  "${diff_status}" in
    0)
      ;;
    1)
      local not_expected_file=""
      mk_temp_file not_expected_file
      set +e
      grep '^[<]' "${diff_file}" > "${not_expected_file}"
      local grep_status="$?"
      set -eo pipefail
      case "${grep_status}" in
        0)
          log_error "Missing output files detected"
          log_file "${not_expected_file}"
          log "Expected outputs listed in ${expected_outputs_file}"
          log "Actual outputs listed in ${actual_outputs_file}"
          exit 1
          ;;
        1)
          ;;
        *)
          log_error "Unexpected status code from grep while checking outputs: ${grep_status}"
          exit 1
          ;;
      esac
      # TODO: Emit warning if extra outputs touched.
    # rm "${not_expected_file}"
      ;;
    *)
      log_error "Unexpected status code from diff: ${diff_status}"
      exit 1
      ;;
  esac
# rm "${expected_outputs_file}"
# rm "${actual_outputs_file}"
# rm "${diff_file}"
}

copy_outputs_out_of_sandbox () {
  # cooos stands for copy_outputs_out_of_sandbox.
  local -n cooos_sandbox_dir="$1"
  local -n cooos_listing_file="$2"
  while IFS="" read -r src || [ -n "$src" ]; do
    if [[ "${src}" == "" ]]; then
      continue
    fi
    local dst=$(sed -e "s/^.\{${#cooos_sandbox_dir}\}//" <<< "${src}")
    local dst_dir=$(dirname "${dst}")
    if [[ ! -d "${dst_dir}" ]]; then
      mkdir -p "${dst_dir}"
    fi
    cp "${src}" "${dst}"
  done < "${cooos_listing_file}"
}

# Main function for executing a command via artifact-executor. The environment variables for
# execution are in the associative array $1; the program to execute is $2; arguments are in the
# array $3; the expected inputs is the array $4; the expected outputs is the array $5; the cache
# directory to use is $6 or ${ARTIFACT_EXECUTOR_CACHE}; the mapping function to massage the
# environment according to a sandbox directory is $7 or `rebase_paths_in_environment`.
artifact_execute () {
  # ae stands for artifact_execute.
  local -n ae_env_map="$1"
  local -n ae_program="$2"
  local -n ae_args_array="$3"
  local -n ae_inputs_array="$4"
  local -n ae_outputs_array="$5"

  set +e
  local -n ae_cache_dir_ref="$6" > /dev/null 2>&1
  set -eo pipefail
  if [[ "${ae_cache_dir_ref}" == "" ]]; then
    if [[ "${ARTIFACT_EXECUTOR_CACHE}" == "" ]]; then
      log_error "Artifact executor cache directory unset; pass it as a parameter to artifact_execute or set the environment variable ARTIFACT_EXECUTOR_CACHE"
      exit 1
    else
      local ae_cache_dir="${ARTIFACT_EXECUTOR_CACHE}"
    fi
  else
    local ae_cache_dir="${ae_cache_dir_ref}"
  fi

  set +e
  local -n ae_env_func_ref="$7" > /dev/null 2>&1
  set -eo pipefail
  if [[ "${ae_env_func_ref}" == "" ]]; then
    local ae_env_func=rebase_paths_in_environment

  else
    local ae_env_func="${ae_env_func_ref}"
  fi

  local action_summary=""
  summarize_action ae_program ae_args_array action_summary

  log_debug "Analyzing action: ${action_summary}"

  # Ensure cache directory is in place.
  # TODO: This should be the responsibility of the calling context.
  local sha256_dir="${ae_cache_dir}/sha256"
  local path_dir="${ae_cache_dir}/path"
  local wepai_dir="${ae_cache_dir}/wd_env_pogram_args_inputs_sha256"
  mkdir -p "${sha256_dir}"
  mkdir -p "${path_dir}"
  mkdir -p "${wepai_dir}"

  log_debug "Ensured cache directories under ${ae_cache_dir}: ${action_summary}"

  # Cache working directory.
  local wd=$(pwd)
  local wd_file=""
  mk_temp_file wd_file
  printf '%s\n' > "${wd_file}"
  local wd_file_hash=""
  sha256_file wd_file wd_file_hash
  local cached_wd_file="${sha256_dir}/${wd_file_hash}"
  mv_file wd_file cached_wd_file

  log_debug "Cached working directory in ${cached_wd_file}: ${action_summary}"

  # Cache environment.
  local -a env_array=()
  map_to_array ae_env_map env_array
  local env_file_unsorted=""
  mk_temp_file env_file_unsorted
  local env_file=""
  mk_temp_file env_file
  append_array_to_file env_array env_file_unsorted
  sort "${env_file_unsorted}" > "${env_file}"
# rm "${env_file_unsorted}"
  local env_file_hash=""
  sha256_file env_file env_file_hash
  local cached_env_file="${sha256_dir}/${env_file_hash}"
  mv_file env_file cached_env_file

  log_debug "Cached environment in ${cached_env_file}: ${action_summary}"

  # Cache program.
  local program_hash=""
  local -i program_size=0
  cache_file ae_cache_dir ae_program program_hash program_size
  local cached_program="${sha256_dir}/${program_hash}"

  log_debug "Cached program in ${cached_program}: ${action_summary}"


  # Cache arguments.
  local args_file=""
  mk_temp_file args_file
  append_array_to_file ae_args_array args_file
  local args_file_hash=""
  sha256_file args_file args_file_hash
  local cached_args_file="${sha256_dir}/${args_file_hash}"
  mv_file args_file cached_args_file

  log_debug "Cached arguments in ${cached_args_file}: ${action_summary}"

  # Cache inputs.
  local inputs_file=""
  mk_temp_file inputs_file
  (
    printf '%s\n' "${program}"
    for input_file in "${ae_inputs_array[@]}"; do
      input_file_path=$(realpath -Ls "${input_file}")
      printf '%s\n' "${input_file_path}"
    done
  ) | sort > "${inputs_file}"

  log_debug "Cached inputs in ${inputs_file}: ${action_summary}"

  # Cache inputs with hashes and sizes.
  local inputs_manifest_file=""
  mk_temp_file inputs_manifest_file
  (
    while IFS="" read -r input_file || [ -n "${input_file}" ]; do
      if [[ "${input_file}" == "" ]]; then
        continue
      fi
      local input_file_hash=""
      local -i input_file_size=0
      cache_file ae_cache_dir input_file input_file_hash input_file_size
      printf '%s|%s|%u\n' "${input_file}" "${input_file_hash}" "${input_file_size}"
    done < "${inputs_file}"
  ) | sort > "${inputs_manifest_file}"

  local inputs_manifest_file_hash=""
  sha256_file inputs_manifest_file inputs_manifest_file_hash
  local cached_inputs_manifest_file="${sha256_dir}/${inputs_manifest_file_hash}"
  mv_file inputs_manifest_file cached_inputs_manifest_file

  log_debug "Cached input manifest in ${cached_inputs_manifest_file}: ${action_summary}"

  # Cache outputs.
  local outputs_file=""
  mk_temp_file outputs_file
  (
    for output_file in "${ae_outputs_array[@]}"; do
      # output_file_path=$(realpath -Ls "${output_file}")
      # printf '%s\n' "${output_file_path}"

      printf '%s\n' "${output_file}"
    done
  ) | sort > "${outputs_file}"

  log_debug "Cached outputs in ${outputs_file}: ${action_summary}"

  # Compute ID of execute function.
  local id="${wd_file_hash}.${env_file_hash}.${program_hash}.${args_file_hash}.${inputs_manifest_file_hash}"
  local id_file=""
  mk_temp_file id_file
  printf '%s\n' "${id}" > "${id_file}"
  local id_hash=""
  sha256_file id_file id_hash
  local cached_id_file="${sha256_dir}/${id_hash}"
  mv_file id_file cached_id_file

  # Copy results from cache or else run and cache computation.
  if [[ -f "${wepai_dir}/${id_hash}" ]]; then
    IFS='|' read -r wdfh efh ph afh imfh tomfh < "${wepai_dir}/${id_hash}"
    if [[ "${wdfh}" != "${wd_file_hash}" ]]; then
      log_error "Working directory hash mismatch: ${wdfh} != ${wd_file_hash}"
      log_error "    working-directory.environment.program.args.inputs file: ${wepai_dir}/${id_hash}"
      exit 1
    fi
    if [[ "${efh}" != "${env_file_hash}" ]]; then
      log_error "Environment hash mismatch: ${efh} != ${env_file_hash}"
      log_error "    working-directory.environment.program.args.inputs file: ${wepai_dir}/${id_hash}"
      exit 1
    fi
    if [[ "${ph}" != "${program_hash}" ]]; then
      log_error "Program hash mismatch: ${ph} != ${program_hash}"
      log_error "    working-directory.environment.program.args.inputs file: ${wepai_dir}/${id_hash}"
      exit 1
    fi
    if [[ "${afh}" != "${args_file_hash}" ]]; then
      log_error "Arguments hash mismatch: ${afh} != ${args_file_hash}"
      log_error "    working-directory.environment.program.args.inputs file: ${wepai_dir}/${id_hash}"
      exit 1
    fi
    if [[ "${imfh}" != "${inputs_manifest_file_hash}" ]]; then
      log_error "Inputs manifest file hash mismatch: ${imfh} != ${inputs_manifest_file_hash}"
      log_error "    working-directory.environment.program.args.inputs file: ${wepai_dir}/${id_hash}"
      exit 1
    fi
    local traced_outputs_manifest_file_hash="${tomfh}"
    if [[ ! -f "${sha256_dir}/${traced_outputs_manifest_file_hash}" ]]; then
      log_error "Outputs manifest file is missing: ${sha256_dir}/${traced_outputs_manifest_file_hash}"
      log_error "    working-directory.environment.program.args.inputs file: ${wepai_dir}/${id_hash}"
      exit 1
    fi

    log "Copying from cache: ${action_summary}"

    while IFS="" read -r file_hash_size || [ -n "${file_hash_size}" ]; do
      IFS='|' read -r file hash size <<< "${file_hash_size}"
      if [[ "${file}" == "" ]]; then
        continue
      fi
      if [[ ! -f "${sha256_dir}/${hash}" ]]; then
        log_error "Failed to locate cached output file ${file} with hash ${hash} and size ${size}"
        exit 1
      fi
      local file_dir=$(dirname "${file}")
      if [[ ! -d file_dir ]]; then
        mkdir -p "${file_dir}"
      fi
      cp "${sha256_dir}/${hash}" "${file}"
    done < "${sha256_dir}/${traced_outputs_manifest_file_hash}"
  else
    log "Executing uncached action: ${action_summary}"

    local sandbox_dir=""
    mk_temp_dir sandbox_dir

    local -A rebased_env_map
    "${ae_env_func}" sandbox_dir ae_env_map rebased_env_map
    local -a rebased_env_array=()
    map_to_array rebased_env_map rebased_env_array

    local sandboxed_program=""
    copy_to_sandbox sandbox_dir ae_program sandboxed_program

    local -a sandboxed_inputs
    for input_file in "${ae_inputs_array[@]}"; do
      local sandboxed_input=""
      copy_to_sandbox sandbox_dir input_file sandboxed_input
      sandboxed_inputs+=("${sandboxed_input}")
    done

    local wd="$(pwd)"
    local sandboxed_wd=""
    create_sandboxed_dir sandbox_dir wd sandboxed_wd

    # Compute and cache outputs.
    local fsatrace_output_file=""
    mk_temp_file fsatrace_output_file
    (cd "${sandboxed_wd}" && command env -i - "${rebased_env_array[@]}" "${FSATRACE}" rwmd "${fsatrace_output_file}" -- "${sandboxed_program}" "${ae_args_array[@]}")

    log_debug "Processing input/output events: ${action_summary}"

    # Process events file
    local -A paths_to_kind_states
    while IFS="|" read -r filesystem_event_kind file_path; do
      # Skip empty entries (may occur after last newline) and directories.
      if [[ "${file_path}" == "" || -d "${file_path}" ]]; then
        continue
      fi

      if [[ "${filesystem_event_kind}" == "m" ]]; then
        IFS="|" read -r dst_file_path src_file_path <<< "${file_path}"
        local filesystem_event_kind_1="d"
        transition_fileystem_state_kinds paths_to_kind_states filesystem_event_kind_1 src_file_path
        local filesystem_event_kind_2="w"
        transition_fileystem_state_kinds paths_to_kind_states filesystem_event_kind_2 dst_file_path
      else
        transition_fileystem_state_kinds paths_to_kind_states filesystem_event_kind file_path
      fi
    done < "${fsatrace_output_file}"

    # Gather traced inputs and outputs based on final states.
    local -a traced_inputs
    local -a traced_outputs
    for path in "${!paths_to_kind_states[@]}"; do
      if [[ "${paths_to_kind_states[${path}]}" == "r" ]]; then
        traced_inputs+=("${path}")
      elif [[ "${paths_to_kind_states[${path}]}" == "w" ]]; then
        traced_outputs+=("${path}")
      elif [[ "${paths_to_kind_states[${path}]}" == "rw" ]]; then
        traced_inputs+=("${path}")
        traced_outputs+=("${path}")
      fi
    done

  # rm "${fsatrace_output_file}"

    # Cache traced inputs.
    local traced_inputs_file=""
    mk_temp_file traced_inputs_file
    (
      for traced_input in "${traced_inputs[@]}"; do
        # Accessing `/proc/...` is likely to be ephemeral files related to the running process.
        if [[ "${path}" =~ ^/proc/ ]]; then
          continue
        fi

        printf '%s\n' "${traced_input}"
      done
    ) | sort > "${traced_inputs_file}"
    local traced_inputs_file_hash=""
    sha256_file traced_inputs_file traced_inputs_file_hash
    local cached_traced_inputs_file="${sha256_dir}/${traced_inputs_file_hash}"
    mv_file traced_inputs_file cached_traced_inputs_file

    log_debug "Cached traced inputs ${cached_traced_inputs_file}: ${action_summary}"

    # Check that all inputs are inside cached directory.
    check_hermetic_files sandbox_dir traced_inputs_file

    log_debug "Checked inputs hermeticity: ${action_summary}"

    # Cache traced inputs manifest.
    local traced_inputs_manifest_file=""
    mk_temp_file traced_inputs_manifest_file
    (
      for traced_input in "${traced_inputs[@]}"; do
        # Accessing `/proc/...` is likely to be ephemeral files related to the running process.
        if [[ "${traced_input}" =~ ^/proc/ ]]; then
          continue
        fi

        local traced_input_hash=""
        local traced_input_size=0
        cache_file ae_cache_dir traced_input traced_input_hash traced_input_size
        local rebased_traced_input=$(sed -e "s/^.\{${#sandbox_dir}\}//" <<< "${traced_input}")
        printf '%s|%s|%u\n' "${rebased_traced_input}" "${traced_input_hash}" "${traced_input_size}"
      done
    ) | sort > "${traced_inputs_manifest_file}"
    local traced_inputs_manifest_file_hash=""
    sha256_file traced_inputs_manifest_file traced_inputs_manifest_file_hash
    local cached_traced_inputs_manifest_file="${sha256_dir}/${traced_inputs_manifest_file_hash}"
    mv_file traced_inputs_manifest_file cached_traced_inputs_manifest_file

    log_debug "Cached traced input manifest in ${cached_traced_inputs_manifest_file}: ${action_summary}"


    # Cache traced outputs.
    local traced_outputs_file=""
    mk_temp_file traced_outputs_file
    (
      for traced_output in "${traced_outputs[@]}"; do
        printf '%s\n' "${traced_output}"
      done
    ) | sort > "${traced_outputs_file}"
    local traced_outputs_file_hash=""
    sha256_file traced_outputs_file traced_outputs_file_hash
    local cached_traced_outputs_file="${sha256_dir}/${traced_outputs_file_hash}"
    mv_file traced_outputs_file cached_traced_outputs_file

    log_debug "Cached traced outputs in ${cached_traced_outputs_file}: ${action_summary}"

    # Check that all outputs are inside cached directory.
    check_hermetic_files sandbox_dir traced_outputs_file

    log_debug "Checked output hermeticity: ${action_summary}"

    # Cache traced outputs manifest.
    local rebased_traced_outputs=()
    local rebased_traced_outputs_manifest_file=""
    mk_temp_file rebased_traced_outputs_manifest_file
    local traced_outputs_manifest_file=""
    mk_temp_file traced_outputs_manifest_file
    (
      for traced_output in "${traced_outputs[@]}"; do
        local rebased_traced_output=$(sed -e "s/^.\{${#sandbox_dir}\}//" <<< "${traced_output}")
        local traced_output_hash=""
        local traced_output_size=0

        rebased_traced_outputs+=("${rebased_traced_output}")

        # Avoid recomputing caches by caching now before copying out of sandbox. This requires that
        # the optional last parameter to `cache_file` be passed to denote the "actual path" of the
        # source file.
        cache_file ae_cache_dir traced_output traced_output_hash traced_output_size rebased_traced_output
        printf '%s|%s|%u\n' "${rebased_traced_output}" "${traced_output_hash}" "${traced_output_size}"
        printf '%s\n' "${rebased_traced_output}" >> "${rebased_traced_outputs_manifest_file}"
      done
    ) | sort > "${traced_outputs_manifest_file}"
    mapfile -t rebased_traced_outputs < <(sort "${rebased_traced_outputs_manifest_file}")
  # rm "${rebased_traced_outputs_manifest_file}"
    local traced_outputs_manifest_file_hash=""
    sha256_file traced_outputs_manifest_file traced_outputs_manifest_file_hash
    local cached_traced_outputs_manifest_file="${sha256_dir}/${traced_outputs_manifest_file_hash}"
    mv_file traced_outputs_manifest_file cached_traced_outputs_manifest_file

    log_debug "Cached traced output manifest in ${cached_traced_inputs_file}: ${action_summary}"

    # Check for undeclared inputs and/or missing outputs.
    check_traced_inputs_against_declared sandbox_dir inputs_file traced_inputs_file
    check_traced_outputs_against_declared sandbox_dir outputs_file traced_outputs_file

  # rm "${inputs_file}"
  # rm "${traced_inputs_file}"
  # rm "${outputs_file}"

    log_debug "Checked declared vs. actual inputs and outputs: ${action_summary}"

    local results_manifest_file="${wepai_dir}/${id_hash}"
    printf '%s|%s|%s|%s|%s|%s\n' "${wd_file_hash}" "${env_file_hash}" "${program_hash}" "${args_file_hash}" "${inputs_manifest_file_hash}" "${traced_outputs_manifest_file_hash}" > "${results_manifest_file}"

    log_debug "Cached action result summary in ${results_manifest_file}: ${action_summary}"


    copy_outputs_out_of_sandbox sandbox_dir cached_traced_outputs_file

    log_debug "Copied outputs out of sandbox: ${action_summary}"

    # HACK: Avoid recomputing hashes+sizes of output files "next time" by ensuring that the
    # path-based cache files have a timestamp after the newly-copied-out-of-sandbox files.
    #
    # Technically, this is unsound in the case that some other process changed a
    # newly-copied-out-of-sandbox file after it was copied but before `touch` in this loop.
    log_debug "Rebased traced outputs:  ${rebased_traced_outputs[@]}: ${action_summary}"
    for rebased_traced_output in "${rebased_traced_outputs[@]}"; do
      touch "${path_dir}${rebased_traced_output}"
      log_debug "Updated output cache timestamp at ${path_dir}${rebased_traced_output}: ${action_summary}"
    done

    log_debug "Updated output cache timestamps: ${action_summary}"

  # rm "${traced_outputs_file}"

  # rm -rf "${sandbox_dir}"
  fi
}

# declare -A my_environment
# my_environment["PATH"]="/usr/bin:/sbin:/bin"
# declare my_program="/usr/bin/cp"
# declare my_args=("README.md" "README.copy.md")
# declare my_inputs=("README.md")
# declare my_outputs=("README.copy.md")
# artifact_execute my_environment my_program my_args my_inputs my_outputs
