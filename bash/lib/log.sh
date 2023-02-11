#!/usr/bin/env bash

# Copyright 2023 The Artifact Executor Authors. All rights reserved.
# Use of this source code is governed by a Apache-style license that can be
# found in the LICENSE file.

#
# Simple logging mechanism.
#

set -eo pipefail

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
GRAY='\033[0;90m'
NC='\033[0m'

declare -A default_log_level_config=(
  [debug]=""
  [info]=1
  [success]=1
  [file]=1
  [warning]=1
  [error]=1
)
export default_log_level_config

# Support "gnu date" from Mac HomeBrew configurations.
if [[ "$(which gdate)" != "" ]]; then
  now () {
    gdate '+%Y-%m-%d %H:%M:%S.%N'
  }
else
  now () {
    date '+%Y-%m-%d %H:%M:%S.%N'
  }
fi

init_logging () {
  local -n il_config="$1"
  if [[ "${il_config[debug]}" != "" ]]; then
    log_debug () {
      >&2 printf "🐛 [${GRAY}DEBUG    %s${NC}]  $*\n" "$(now)"
    }
  else
    log_debug () {
      :
    }
  fi

  if [[ "${il_config[info]}" != "" ]]; then
    log_info () {
      >&2 printf "ℹ️ [${GRAY}INFO     %s${NC}]  $*\n" "$(now)"
    }
  else
    log_info () {
      :
    }
  fi
  log () {
    log_info "$@"
  }

  local -n il_config="$1"
  if [[ "${il_config[success]}" != "" ]]; then
    log_success () {
      >&2 printf "✔️ [${GREEN}SUCCESS${NC}  %s]  $*\n" "$(now)"
    }
  else
    log_success () {
      :
    }
  fi

  if [[ "${il_config[file]}" != "" ]]; then
    log_file () {
      >&2 printf "📄 [${BLUE}FILE   ${NC}  %s]  $1\n" "$(now)"
      >&2 cat "$1"
    }
  else
    log_file () {
      :
    }
  fi

  if [[ "${il_config[warning]}" != "" ]]; then
    log_warning () {
      >&2 printf "⚠️ [${YELLOW}WARNING${NC}  %s]  $*\n" "$(now)"
    }
  else
    log_warning () {
      :
    }
  fi
  log_warn () {
    log_warning "$@"
  }

  if [[ "${il_config[error]}" != "" ]]; then
    log_error () {
      >&2 printf "🛑 [${RED}ERROR  ${NC}  %s]  $*\n" "$(now)"
    }
  else
    log_error () {
      :
    }
  fi
  log_err () {
    log_error "$@"
  }
}

# log () {
#   if [[ "${log_level_config[]}" ]]
#   >&2 printf "ℹ️ [${GRAY}INFO     %s${NC}]  $*\n" "$(now)"
# }

# log_success () {
#   >&2 printf "✔️ [${GREEN}SUCCESS${NC}  %s]  $*\n" "$(now)"
# }

# log_warning () {
#   >&2 printf "⚠️ [${YELLOW}WARNING${NC}  %s]  $*\n" "$(now)"
# }

# log_error () {
#   >&2 printf "🛑 [${RED}ERROR  ${NC}  %s]  $*\n" "$(now)"
# }

# log_file () {
#   >&2 printf "📄 [${BLUE}FILE   ${NC}  %s]  $1\n" "$(now)"
#   >&2 cat "$1"
# }
