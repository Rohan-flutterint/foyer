//  Copyright 2024 foyer Project Authors
//
//  Licensed under the Apache License, Version 2.0 (the "License");
//  you may not use this file except in compliance with the License.
//  You may obtain a copy of the License at
//
//  http://www.apache.org/licenses/LICENSE-2.0
//
//  Unless required by applicable law or agreed to in writing, software
//  distributed under the License is distributed on an "AS IS" BASIS,
//  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//  See the License for the specific language governing permissions and
//  limitations under the License.

//! Intrusive double linked list implementation.

use std::{marker::PhantomData, ptr::NonNull};

/// Essential data structure to build an intrusive double linked list.
pub struct link {
    prev: Option<NonNull<()>>,
    next: Option<NonNull<()>>,
}

unsafe impl Send for link {}
unsafe impl Sync for link {}

pub trait Pointer {}

pub trait Adapter {
    type Item;

    fn item_to_link(item: NonNull<Self::Item>) -> NonNull<link>;
}

pub struct List<A> {
    _marker: PhantomData<A>,
}
