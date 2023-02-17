// Copyright 2023 The Artifact Executor Authors. All rights reserved.
// Use of this source code is governed by a Apache-style license that can be
// found in the LICENSE file.

use std::process::Command;

fn main() {
    #[cfg(target_os = "linux")]
    {
        let mut child = Command::new("make")
            .current_dir("../fsatrace")
            .spawn()
            .expect("spawn: make fsatrace");
        let status = child.wait().expect("execute: make fsatrace");
        if !status.success() {
            panic!("failed to make fsatrace");
        }
    }
}
