#!/usr/bin/env bash

#
# Common environmenet variables used in other scripts.
#

set -eo pipefail

source "$(dirname ${BASH_SOURCE[0]})/trace.sh"

source "$(dirname ${BASH_SOURCE[0]})/log.sh"
init_logging default_log_level_config

# fsatrace needed for tracking inputs and outputs.
# https://github.com/jacereda/fsatrace
declare -r FSATRACE="$(which fsatrace)"
declare -r FSATRACE_SO=$(dirname "${FSATRACE}")/fsatrace.so

# Convert current environment to associative array for use as a default environment.
declare -A default_environment
while IFS="" read -r binding || [ -n "${binding}" ]; do
  IFS='=' read -r key value <<< "${binding}"
  if [[ "${key}" == "" ]]; then
    continue
  fi
  default_environment["${key}"]="${value}"
done < <(env)
