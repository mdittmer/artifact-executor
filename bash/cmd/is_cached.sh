#!/usr/bin/env bash

# Copyright 2023 The Artifact Executor Authors. All rights reserved.
# Use of this source code is governed by a Apache-style license that can be
# found in the LICENSE file.

#
# Signal (via exit code) whether a file is in the Artifact Executor cache.
#

set -eo pipefail

declare cache_dir=""
declare input_file=""
declare real_path=""

while [[ $# -gt 0 ]]; do
  case $1 in
    -c|--cache|--cache-dir)
      cache_dir=$(realpath -Ls "$2")
      shift
      shift
      ;;
    -f|-i|--file|--input)
      input_file="$2"
      shift
      shift
      ;;
    *)
      >&2 printf 'Unrecognized is-cached argument: "%s"\n' "$1"
      exit 1
      ;;
  esac
done

CACHE_DIR="${ARTIFACT_EXECUTOR_CACHE:-${cache_dir}}"
if [[ "${CACHE_DIR}" == "" ]]; then
  >&2 printf 'Missing cache directory; either:\nSet environment variable ${CACHE_DIR}, or\nPass directory path via -c|--cache|--cache-dir\n'
  exit 1
fi

if [[ "${input_file}" == "" ]]; then
  >&2 printf 'Missing input file via -f|-i|--file|--input\n'
  exit 1
fi
if [[ ! -f "${input_file}" ]]; then
  >&2 printf 'Input file does not exist: "%s"\n' "${input_file}"
  exit 1
fi

declare sha256_dir="${CACHE_DIR}/sha256"
declare path_dir="${CACHE_DIR}/path"
declare wepai_dir="${CACHE_DIR}/wd_env_pogram_args_inputs_sha256"
mkdir -p "${sha256_dir}"
mkdir -p "${path_dir}"
mkdir -p "${wepai_dir}"

CMD_DIR=$(dirname "${BASH_SOURCE[0]}")
LIB_DIR="${CMD_DIR}/../lib"

source "${LIB_DIR}/lib.sh"

source "${LIB_DIR}/log.sh"
init_logging default_log_level_config

declare is_cached_result=""
is_file_cached CACHE_DIR input_file is_cached_result
if [[ "${is_cached_result}" == "true" ]]; then
  log_debug "File is cached ${input_file}"
  exit 0
else
  log_debug "File is not cached ${input_file}"
  exit 1
fi
