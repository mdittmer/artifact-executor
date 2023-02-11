#!/usr/bin/env bash

# Copyright 2023 The Artifact Executor Authors. All rights reserved.
# Use of this source code is governed by a Apache-style license that can be
# found in the LICENSE file.

#
# Core implementation of best-effort tracing environment variable accesses.
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
artifact_trace_environment () {
  # ate stands for artifact_trace_environment.
  local -n ate_env_map="$1"
  local -n ate_program="$2"
  local -n ate_args_array="$3"
  local -n ate_output_env_map="$4"

  set +e
  local -n ate_cache_dir_ref="$6" > /dev/null 2>&1
  set -eo pipefail
  if [[ "${ate_cache_dir_ref}" == "" ]]; then
    if [[ "${ARTIFACT_EXECUTOR_CACHE}" == "" ]]; then
      log_error "Artifact executor cache directory unset; pass it as a parameter to artifact_execute or set the environment variable ARTIFACT_EXECUTOR_CACHE"
      exit 1
    else
      local ate_cache_dir="${ARTIFACT_EXECUTOR_CACHE}"
    fi
  else
    local ate_cache_dir="${ate_cache_dir_ref}"
  fi

  # Ensure cache directory is in place.
  local sha256_dir="${ate_cache_dir}/sha256"
  local wepai_dir="${ate_cache_dir}/wd_env_pogram_args_inputs_sha256"
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
  map_to_array ate_env_map env_array
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
  sha256_file ate_program program_hash
  local program=$(realpath -Ls "${ate_program}")
  local cached_program="${sha256_dir}/${program_hash}"
  cp "${program}" "${cached_program}"
  chmod u+w "${cached_program}"

  # Cache arguments.
  local args_file=""
  mk_temp_file args_file
  append_array_to_file ate_args_array args_file
  local args_file_hash=""
  sha256_file args_file args_file_hash
  local cached_args_file="${sha256_dir}/${args_file_hash}"
  mv_file args_file cached_args_file

  # Copy results from cache or else run and cache computation.
  log "Tracing action environment"

  # Execute ltrace and store environment variables that were actually set.
  while IFS="" read -r env_var || [ -n "${env_var}" ]; do
    if [[ "${env_var}" == "" ]]; then
      continue
    fi

    if [[ "${ate_env_map[${env_var}]}" != "" ]]; then
      ate_output_env_map["${env_var}"]="${ate_env_map[${env_var}]}"
    fi
  done < <(
    (
      command env -i - "${env_array[@]}" ltrace -e getenv "${ate_program}" "${ate_args_array[@]}"
    ) |& grep -o 'getenv("[^"]\+")' | cut -d '"' -f2 | sort | uniq
  )
}

# declare -A my_environment
# my_environment["PATH"]="/usr/bin:/sbin:/bin"
# declare my_program="/usr/bin/cp"
# declare my_args=("README.md" "README.copy.md")
# declare -a my_inputs
# declare -a my_outputs
# declare -A my_exercised_environment
# artifact_trace_environment my_environment my_program my_args my_exercised_environment

# echo "EXERCISED ENV KEYS: ${!my_exercised_environment[@]}"
# echo "EXERCISED ENV VALUES: ${my_exercised_environment[@]}"
