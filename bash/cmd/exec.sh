#!/usr/bin/env bash

#
# Execute a binary via Artifact Executor.
#

declare cache_dir=""
declare environment_manifest_file=""
declare program=""
declare args_manifest_file=""
declare inputs_manifest_file=""
declare outputs_manifest_file=""

while [[ $# -gt 0 ]]; do
  case $1 in
    -c|--cache|--cache-dir)
      cache_dir=$(realpath -Ls "$2")
      shift
      shift
      ;;
    -e|--env|--environment|--environment-manifest)
      environment_manifest_file=$(realpath -Ls "$2")
      shift
      shift
      ;;
    -p|--program|--executable)
      program=$(realpath -Ls "$2")
      shift
      shift
      ;;
    -a|--args|--arguments|--argments-manifest|--argments-manifest)
      args_manifest_file=$(realpath -Ls "$2")
      shift
      shift
      ;;
    -i|--inputs|--inputs-manifest|--input-manifest)
      inputs_manifest_file=$(realpath -Ls "$2")
      shift
      shift
      ;;
    -o|--outputs|--outputs-manifest|--output-manifest)
      outputs_manifest_file=$(realpath -Ls "$2")
      shift
      shift
      ;;
    *)
      2> printf 'Unrecognized exec argument: "%s"\n' "$1"
      exit 1
      ;;
  esac
done

CACHE_DIR="${ARTIFACT_EXECUTOR_CACHE:-${cache_dir}}"
if [[ "${CACHE_DIR}" == "" ]]; then
  >&2 printf 'Missing cache directory; either:\nSet environment variable ${CACHE_DIR}, or\nPass directory path via -c|--cache|--cache-dir\n'
  exit 1
fi

declare -A ENV_MAP=()
if [[ "${#ARTIFACT_EXECUTOR_ENV}" == "0" ]]; then
  if [[ "${environment_manifest_file}" == "" ]]; then
    >&2 printf 'Missing environment map; either:\nSet environment variable associative array ${ARTIFACT_EXECUTOR_ENV}, or\nPass file path via -e|--env|--environment|--environment-manifest\n'
    exit 1
  elif [[ ! -f "${environment_manifest_file}" ]]; then
    >&2 printf 'Environment map file not found: %s\n' "${environment_manifest_file}"
    exit 1
  else
    while IFS="=" read -r env_key env_value || [[ -n "${env_key}" ]]; do
      if [[ "${env_key}" == "" ]]; then
        continue
      fi
      ENV_MAP[${env_key}]="${env_value}"
    done < "${environment_manifest_file}"
  fi
else
  for env_key in "${!ARTIFACT_EXECUTOR_ENV[@]}"; do
    ENV_MAP[${env_key}]="${ARTIFACT_EXECUTOR_ENV[${env_key}]}"
  done
fi

declare PROGRAM="${ARTIFACT_EXECUTOR_PROGRAM:-${program}}"
if [[ "${PROGRAM}" == "" ]]; then
  >&2 printf 'Missing program; either:\nSet environment variable ${ARTIFACT_EXECUTOR_PROGRAM}, or\nPass directory path via -p|--program|--executable\n'
  exit 1
fi

declare -a ARGS_ARRAY=()
if [[ "${#ARTIFACT_EXECUTOR_ARGS}" == "0" ]]; then
  if [[ "${args_manifest_file}" == "" ]]; then
    >&2 printf 'Missing arguments array; either:\nSet environment variable array ${ARTIFACT_EXECUTOR_ARGS}, or\nPass file path via -a|--args|--arguments|--argments-manifest|--argments-manifest\n'
    exit 1
  elif [[ ! -f "${args_manifest_file}" ]]; then
    >&2 printf 'Arguments array file not found: %s\n' "${args_manifest_file}"
    exit 1
  else
    while IFS="" read -r arg || [[ -n "${arg}" ]]; do
      ARGS_ARRAY+=("${arg}")
    done < "${args_manifest_file}"
  fi
else
  for arg in "${ARTIFACT_EXECUTOR_ARGS[@]}"; do
    ARGS_ARRAY+=("${arg}")
  done
fi

declare -a INPUTS_ARRAY=()
if [[ "${#ARTIFACT_EXECUTOR_INPUTS}" == "0" ]]; then
  if [[ "${inputs_manifest_file}" == "" ]]; then
    >&2 printf 'Missing inputs array; either:\nSet environment variable array ${ARTIFACT_EXECUTOR_INPUTS}, or\nPass file path via -a|--args|--arguments|--argments-manifest|--argments-manifest\n'
    exit 1
  elif [[ ! -f "${inputs_manifest_file}" ]]; then
    >&2 printf 'Inputs array file not found: %s\n' "${inputs_manifest_file}"
    exit 1
  else
    while IFS="" read -r input || [[ -n "${input}" ]]; do
      INPUTS_ARRAY+=("${input}")
    done < "${inputs_manifest_file}"
  fi
else
  for arg in "${ARTIFACT_EXECUTOR_INPUTS[@]}"; do
    INPUTS_ARRAY+=("${arg}")
  done
fi

declare -a OUTPUTS_ARRAY=()
if [[ "${#ARTIFACT_EXECUTOR_OUTPUTS}" == "0" ]]; then
  if [[ "${outputs_manifest_file}" == "" ]]; then
    >&2 printf 'Missing outputs array; either:\nSet environment variable array ${ARTIFACT_EXECUTOR_OUTPUTS}, or\nPass file path via -a|--args|--arguments|--argments-manifest|--argments-manifest\n'
    exit 1
  elif [[ ! -f "${outputs_manifest_file}" ]]; then
    >&2 printf 'Outputs array file not found: %s\n' "${outputs_manifest_file}"
    exit 1
  else
    while IFS="" read -r output || [[ -n "${output}" ]]; do
      OUTPUTS_ARRAY+=("${output}")
    done < "${outputs_manifest_file}"
  fi
else
  for arg in "${ARTIFACT_EXECUTOR_OUTPUTS[@]}"; do
    OUTPUTS_ARRAY+=("${arg}")
  done
fi

CMD_DIR=$(dirname "${BASH_SOURCE[0]}")
LIB_DIR="${CMD_DIR}/../lib"

source "${LIB_DIR}/artifact_execute.sh"
source "${LIB_DIR}/lib.sh"

source "${LIB_DIR}/log.sh"
init_logging default_log_level_config


declare action_summary=""
summarize_action PROGRAM ARGS_ARRAY action_summary
log "Starting  ${action_summary}"
artifact_execute ENV_MAP PROGRAM ARGS_ARRAY INPUTS_ARRAY OUTPUTS_ARRAY CACHE_DIR
log_success "Completed  ${action_summary}"
