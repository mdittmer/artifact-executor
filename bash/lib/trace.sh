#!/usr/bin/env bash

# Copyright 2023 The Artifact Executor Authors. All rights reserved.
# Use of this source code is governed by a Apache-style license that can be
# found in the LICENSE file.

#
# State machine transitions for observing a stream of fsatrace filesystem events.
#

# Events (see fsatrace documentation for details):
# - r|path: read file at path;
# - w|path: write file at path;
# - d|path: delete file at path;
# - m|dst|src: move file from src to dst.
#
# States:
# - r: File is only ever read;
# - w: File is first written, then possibly read, but not delted, or, if it is
#      deleted, it is subsequently written again;
# - rw: File is read before also being written/read/deleted;
# - d: File is first written, then possibly read/written, then deleted.

transition_fileystem_state_kinds () {
  # tfsk stands for transition_fileystem_state_kinds.
  local -n kinds=$1
  local -n kind=$2
  local -n path=$3
  case "${kinds[${path}]}" in
    "")
      case "${kind}" in
        r)
          kinds["${path}"]="r"
          ;;
        w)
          kinds["${path}"]="w"
          ;;
        d)
          log "Command trace contains delete before write pattern on path \"${path}\""
          exit 1
          ;;
        *)
          log "Command trace contains unknown fileystem event kind \"${kinds[${path}]}\""
          exit 1
          ;;
      esac
      ;;
    r)
      case "${kind}" in
        r)
          kinds["${path}"]="r"
          ;;
        w)
          kinds["${path}"]="rw"
          ;;
        d)
          log "Command trace contains read-then-delete pattern on path \"${path}\""
          exit 1
          ;;
        *)
          log "Command trace contains unknown fileystem event kind \"${kinds[${path}]}\""
          exit 1
          ;;
      esac
      ;;
    w)
      case "${kind}" in
        r)
          kinds["${path}"]="w"
          ;;
        w)
          kinds["${path}"]="w"
          ;;
        d)
          kinds["${path}"]="d"
          ;;
        *)
          log "Command trace contains unknown fileystem event kind \"${kinds[${path}]}\""
          exit 1
          ;;
      esac
      ;;
    rw)
      case "${kind}" in
        r)
          kinds["${path}"]="rw"
          ;;
        w)
          kinds["${path}"]="rw"
          ;;
        d)
          kinds["${path}"]="rw"
          ;;
        *)
          log "Command trace contains unknown fileystem event kind \"${kinds[${path}]}\""
          exit 1
          ;;
      esac
      ;;
    d)
      case "${kind}" in
        r)
          log "Command trace contains delete-then-read pattern on path \"${path}\""
          exit 1
          ;;
        w)
          kinds["${path}"]="w"
          ;;
        d)
          log "Command trace contains delete-then-delete pattern on path \"${path}\""
          exit 1
          ;;
        *)
          log "Command trace contains unknown fileystem event kind \"${kinds[${path}]}\""
          exit 1
          ;;
      esac
      ;;
    *)
      log "Command trace state contains unknown state value \"${kinds[${path}]}\""
      exit 1
      ;;
  esac
}