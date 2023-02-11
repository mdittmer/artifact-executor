#!/usr/bin/env bash

# Copyright 2023 The Artifact Executor Authors. All rights reserved.
# Use of this source code is governed by a Apache-style license that can be
# found in the LICENSE file.

set -eo pipefail

#
# Store the current state of a file in the Artifact Executor cache.
#

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
      input_file=$(realpath -Ls "$2")
      shift
      shift
      ;;
    -p|-r|--path|--real-path)
      real_path=$(realpath -Ls "$2")
      shift
      shift
      ;;
    *)
      >&2 printf 'Unrecognized cache-file argument: "%s"\n' "$1"
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

declare hash=""
declare -i size=0
if [[ "${real_path}" == "" ]]; then
  cache_file CACHE_DIR input_file hash size
  log_debug "Cached ${input_file}|${hash}|${size}"
else
  cache_file CACHE_DIR input_file hash size real_path
  log_debug "Cached ${input_file} as ${real_path}|${hash}|${size}"
fi

if [[ "${hash}" == "" ]]; then
  >&2 printf 'Failed to compute hash for file: "%s"\n' "${input_file}"
  exit 1
fi

# printf '%s|%s|%u\n' "${input_file}" "${hash}" "${size}"
