// Copyright (C) 2019-2023 Aleo Systems Inc.
// This file is part of the snarkVM library.

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at:
// http://www.apache.org/licenses/LICENSE-2.0

// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

mod bytes;
mod parse;
mod serialize;

use crate::{Identifier, U32};

use snarkvm_console_network::prelude::*;

/// A register `Access`.
#[derive(Clone, PartialEq, Eq, Hash)]
pub enum Access<N: Network> {
    /// The access is an index.
    Index(U32<N>),
    /// The access is a member.
    Member(Identifier<N>),
}
