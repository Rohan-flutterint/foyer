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

//! Macros for deriving essential components to build an intrusive data structures.

use darling::FromDeriveInput;
use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput};

/// Derive adapters for intrusive double linked list.
#[proc_macro_derive(IntrusiveList, attributes(item, linker))]
pub fn derive_intrusive_list(input: TokenStream) -> TokenStream {
    let input: DeriveInput = parse_macro_input!(input);

    println!("input ==========> {input:#?}");

    println!("ident ==========> {:#?}", input.ident);

    TokenStream::new()
}

// pub struct Record<K, V> {
//     key: K,
//     value: V,
//     state: State<K, V>,
// }

// #[derive(IntrusiveList)]
// #[item(Record)]
// pub struct State<K, V> {
//     val: u64,
//     #[link]
//     link: Link,
// }
