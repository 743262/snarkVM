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

mod boolean;
mod field;
mod integer;
mod scalar;

use crate::prelude::*;

use snarkvm_console_types_integers::Integer;

/// Unary operator for casting values of one type to another, with lossy truncation.
pub trait CastLossy<T: Sized = Self> {
    /// Casts the value of `self` into a value of type `T`, with lossy truncation.
    ///
    /// This method makes a *best-effort* attempt to preserve all bits of information,
    /// but it is not guaranteed to do so.
    fn cast_lossy(&self) -> Result<T>;
}
