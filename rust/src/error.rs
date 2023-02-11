// Copyright 2023 The Artifact Executor Authors. All rights reserved.
// Use of this source code is governed by a Apache-style license that can be
// found in the LICENSE file.

pub trait Error: 'static + std::error::Error + Send + Sync {}

impl<T: 'static + std::error::Error + Send + Sync> Error for T {}
