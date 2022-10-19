#!/usr/bin/env bash

#
# Core implementation of tracing input and output files.
#

set -eo pipefail

declare LIB_DIR="$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
source "${LIB_DIR}/env.sh"
source "${LIB_DIR}/lib.sh"
source "${LIB_DIR}/trace.sh"

source "${LIB_DIR}/log.sh"
init_logging default_log_level_config

# Main function for tracing inputs/outputs via artifact-executor. The environment variables for
# execution are in the associative array $1; the program to execute is $2; arguments are in the
# array $3; the inputs will be added to the array $4; the outputs will be added to the array $5;
# the cache directory to use is $6 or ${ARTIFACT_EXECUTOR_CACHE}.
artifact_trace_inputs_outputs () {
  # atio stands for artifact_trace_inputs_outputs.
  local -n atio_env_map="$1"
  local -n atio_program="$2"
  local -n atio_args_array="$3"
  local -n atio_inputs_array="$4"
  local -n atio_outputs_array="$5"

  set +e
  local -n atio_cache_dir_ref="$6" > /dev/null 2>&1
  set -eo pipefail
  if [[ "${atio_cache_dir_ref}" == "" ]]; then
    if [[ "${ARTIFACT_EXECUTOR_CACHE}" == "" ]]; then
      log_error "Artifact executor cache directory unset; pass it as a parameter to artifact_execute or set the environment variable ARTIFACT_EXECUTOR_CACHE"
      exit 1
    else
      local atio_cache_dir="${ARTIFACT_EXECUTOR_CACHE}"
    fi
  else
    local atio_cache_dir="${atio_cache_dir_ref}"
  fi

  # Ensure cache directory is in place.
  local sha256_dir="${atio_cache_dir}/sha256"
  local wepai_dir="${atio_cache_dir}/wd_env_pogram_args_inputs_sha256"
  mkdir -p "${sha256_dir}"
  mkdir -p "${wepai_dir}"

  # Cache working directory.
  local wd=$(pwd)
  local wd_file=""
  mk_temp_file wd_file
  printf '%s\n' > "${wd_file}"
  local wd_file_hash=""
  sha256_file wd_file wd_file_hash
  local cached_wd_file="${sha256_dir}/${wd_file_hash}"
  mv_file wd_file cached_wd_file

  # Cache environment.
  local -a env_array=()
  map_to_array atio_env_map env_array
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

  # Cache program.
  local program_hash=""
  sha256_file atio_program program_hash
  local program=$(realpath -Ls "${atio_program}")
  local cached_program="${sha256_dir}/${program_hash}"
  cp "${program}" "${cached_program}"
  chmod u+w "${cached_program}"

  # Cache arguments.
  local args_file=""
  mk_temp_file args_file
  append_array_to_file atio_args_array args_file
  local args_file_hash=""
  sha256_file args_file args_file_hash
  local cached_args_file="${sha256_dir}/${args_file_hash}"
  mv_file args_file cached_args_file

  # Copy results from cache or else run and cache computation.
  log "Tracing action inputs and outputs"

  # Execute trace.
  fsatrace_output_file=""
  mk_temp_file fsatrace_output_file
  (command env -i - "${env_array[@]}" "${FSATRACE}" rwmd "${fsatrace_output_file}" -- "${atio_program}" "${atio_args_array[@]}")

  # Process events file.
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
    # Accessing `/proc/...` is likely to be ephemeral files related to the running process.
    if [[ "${path}" =~ ^/proc/ ]]; then
      continue
    fi
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
      printf '%s\n' "${traced_input}"
    done
  ) | sort > "${traced_inputs_file}"
  local traced_inputs_file_hash=""
  sha256_file traced_inputs_file traced_inputs_file_hash
  local cached_traced_inputs_file="${sha256_dir}/${traced_inputs_file_hash}"
  mv_file traced_inputs_file cached_traced_inputs_file

  # Cache traced inputs manifest.
  local traced_inputs_manifest_file=""
  mk_temp_file traced_inputs_manifest_file
  (
    for traced_input in "${traced_inputs[@]}"; do
      local traced_input_hash=""
      sha256_file traced_input traced_input_hash
      printf '%s=%s\n' "${traced_input}" "${traced_input_hash}"

      # cp because not operating inside a sandbox.
      cp "${traced_input}" "${sha256_dir}/${traced_input_hash}"
      chmod u+w "${sha256_dir}/${traced_input_hash}"
    done
  ) | sort > "${traced_inputs_manifest_file}"
  local traced_inputs_manifest_file_hash=""
  sha256_file traced_inputs_manifest_file traced_inputs_manifest_file_hash
  local cached_traced_inputs_manifest_file="${sha256_dir}/${traced_inputs_manifest_file_hash}"
  mv_file traced_inputs_manifest_file cached_traced_inputs_manifest_file

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

  # Cache traced outputs manifest.
  local traced_outputs_manifest_file=""
  mk_temp_file traced_outputs_manifest_file
  (
    for traced_output in "${traced_outputs[@]}"; do
      local traced_output_hash=""
      sha256_file traced_output traced_output_hash
      printf '%s=%s\n' "${traced_output}" "${traced_output_hash}"

      # cp because not operating inside a sandbox.
      cp "${traced_output}" "${sha256_dir}/${traced_output_hash}"
      chmod u+w "${sha256_dir}/${traced_output_hash}"
    done
  ) | sort > "${traced_outputs_manifest_file}"
  local traced_outputs_manifest_file_hash=""
  sha256_file traced_outputs_manifest_file traced_outputs_manifest_file_hash
  local cached_traced_outputs_manifest_file="${sha256_dir}/${traced_outputs_manifest_file_hash}"
  mv_file traced_outputs_manifest_file cached_traced_outputs_manifest_file

  # Compute ID of execute function.
  local id="${wd_file_hash}.${env_file_hash}.${program_hash}.${args_file_hash}.${traced_inputs_manifest_file_hash}"
  local id_file=""
  mk_temp_file id_file
  printf '%s\n' "${id}" > "${id_file}"
  local id_hash=""
  sha256_file id_file id_hash
  local cached_id_file="${sha256_dir}/${id_hash}"
  mv_file id_file cached_id_file

  # cp because already moved to cache.
  local results_manifest_file="${wepai_dir}/${id_hash}"
  cp "${traced_outputs_manifest_file}" "${results_manifest_file}"
  chmod u+w "${results_manifest_file}"

  # Copy execution inputs to function's output array.
  while IFS="" read -r input_file || [ -n "${input_file}" ]; do
    if [[ "${input_file}" == "" ]]; then
      continue
    fi
    atio_inputs_array+=("${input_file}")
  done < "${traced_inputs_file}"

  # Copy execution outputs to function's output array.
  while IFS="" read -r output_file || [ -n "${output_file}" ]; do
    if [[ "${output_file}" == "" ]]; then
      continue
    fi
    atio_outputs_array+=("${output_file}")
  done < "${traced_outputs_file}"
}

# declare -A my_environment
# my_environment["PATH"]="/usr/bin:/sbin:/bin"
# declare my_program="/usr/bin/cp"
# declare my_args=("README.md" "README.copy.md")
# declare -a my_inputs
# declare -a my_outputs
# artifact_trace_inputs_outputs my_environment my_program my_args my_inputs my_outputs

# echo "INPUTS: ${my_inputs[@]}"
# echo "OUTPUTS: ${my_outputs[@]}"
