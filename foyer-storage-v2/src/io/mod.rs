// Copyright 2025 foyer Project Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use buffer::allocator::AlignedAllocator;

mod buffer;
mod device;
mod driver;
mod engine;
mod error;
mod hub;
mod task;

/// IO operation alignment.
pub const IO_ALIGN: usize = 4096;
/// IO buffer allocator.
pub const IO_BUFFER_ALLOCATOR: AlignedAllocator<IO_ALIGN> = AlignedAllocator::new();
