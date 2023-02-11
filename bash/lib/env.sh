#!/usr/bin/env bash

# Copyright 2023 The Artifact Executor Authors. All rights reserved.
# Use of this source code is governed by a Apache-style license that can be
# found in the LICENSE file.

#
# Common environmenet variables used in other scripts.
#

set -eo pipefail

declare LIB_DIR="$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
source "${LIB_DIR}/trace.sh"

source "${LIB_DIR}/log.sh"
init_logging default_log_level_config

# fsatrace vendored in ../../fsatrace/.
declare FSATRACE_DIR="$(dirname -- $(dirname -- "${LIB_DIR}"))/fsatrace"
(cd "${FSATRACE_DIR}" && make) > /dev/null 2>&1

declare FSATRACE="${FSATRACE_DIR}/fsatrace"
declare FSATRACE_SO="${FSATRACE_DIR}/fsatrace.so"

# Convert current environment to associative array for use as a default environment.
declare -A default_environment
while IFS="" read -r binding || [ -n "${binding}" ]; do
  IFS='=' read -r key value <<< "${binding}"
  if [[ "${key}" == "" ]]; then
    continue
  fi
  default_environment["${key}"]="${value}"
done < <(env)
