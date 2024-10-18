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

use std::{
    marker::PhantomData,
    ptr::NonNull,
    sync::atomic::{AtomicU64, AtomicUsize, Ordering},
};

use bitflags::bitflags;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct RecordFlags: u64 {
        const IN_INDEXER = 0b00000001;
        const IN_EVICTION = 0b00000010;
        const DEPOSIT= 0b00000100;
    }
}

pub struct RecordData<K, V, H, S> {
    pub key: K,
    pub value: V,
    pub hint: H,
    pub state: S,
    pub hash: u64,
    pub weight: usize,
}

/// [`Record`] is a continuous piece of heap allocated memory, stores per entry data.
///
/// The lifetime of [`Record`] is managed by foyer. It can only be accessed by [`RecordResolver`] with [`RecordToken`].
pub struct Record<K, V, H, S> {
    data: Option<RecordData<K, V, H, S>>,
    refs: AtomicUsize,
    flags: AtomicU64,
}

impl<K, V, H, S> Record<K, V, H, S> {
    pub fn new() -> Self {
        Self {
            data: None,
            refs: AtomicUsize::new(0),
            flags: AtomicU64::new(0),
        }
    }

    pub fn set(&mut self, data: RecordData<K, V, H, S>) {
        let old = self.data.replace(data);
        assert!(old.is_none());
    }

    pub fn take(&mut self) -> RecordData<K, V, H, S> {
        self.data.take().unwrap()
    }

    pub fn key(&self) -> &K {
        &self.data.as_ref().unwrap().key
    }

    pub fn value(&self) -> &V {
        &self.data.as_ref().unwrap().value
    }

    pub fn hint(&self) -> &H {
        &self.data.as_ref().unwrap().hint
    }

    pub fn state(&self) -> &S {
        &self.data.as_ref().unwrap().state
    }

    pub fn state_mut(&mut self) -> &mut S {
        &mut self.data.as_mut().unwrap().state
    }

    pub fn hash(&self) -> u64 {
        self.data.as_ref().unwrap().hash
    }

    pub fn weight(&self) -> usize {
        self.data.as_ref().unwrap().weight
    }

    pub fn refs(&self) -> &AtomicUsize {
        &self.refs
    }

    pub fn flags(&self) -> &AtomicU64 {
        &self.flags
    }

    pub fn set_in_indexer(&self, val: bool, order: Ordering) {
        self.set_flags(RecordFlags::IN_INDEXER, val, order);
    }

    pub fn is_in_indexer(&self, order: Ordering) -> bool {
        self.get_flags(RecordFlags::IN_INDEXER, order)
    }

    pub fn set_in_eviction(&self, val: bool, order: Ordering) {
        self.set_flags(RecordFlags::IN_EVICTION, val, order);
    }

    pub fn is_in_eviction(&self, order: Ordering) -> bool {
        self.get_flags(RecordFlags::IN_EVICTION, order)
    }

    pub fn set_deposit(&self, val: bool, order: Ordering) {
        self.set_flags(RecordFlags::DEPOSIT, val, order);
    }

    pub fn is_deposit(&self, order: Ordering) -> bool {
        self.get_flags(RecordFlags::DEPOSIT, order)
    }

    pub fn set_flags(&self, flags: RecordFlags, val: bool, order: Ordering) {
        match val {
            true => self.flags.fetch_or(flags.bits(), order),
            false => self.flags.fetch_and(!flags.bits(), order),
        };
    }

    pub fn get_flags(&self, flags: RecordFlags, order: Ordering) -> bool {
        self.flags.fetch_and(flags.bits(), order) != 0
    }
}

pub struct RecordToken<K, V, H, S> {
    ptr: NonNull<Record<K, V, H, S>>,
}

unsafe impl<K, V, H, S> Send for RecordToken<K, V, H, S> {}
unsafe impl<K, V, H, S> Sync for RecordToken<K, V, H, S> {}

impl<K, V, H, S> Clone for RecordToken<K, V, H, S> {
    fn clone(&self) -> Self {
        Self { ptr: self.ptr }
    }
}

impl<K, V, H, S> Copy for RecordToken<K, V, H, S> {}

impl<K, V, H, S> PartialEq for RecordToken<K, V, H, S> {
    fn eq(&self, other: &Self) -> bool {
        self.ptr == other.ptr
    }
}

impl<K, V, H, S> Eq for RecordToken<K, V, H, S> {}

/// [`RecordResolver`] resolves [`RecordToken`] to immutable or mutable reference of [`Record`].
///
/// [`RecordResolver`] doesn't do anything buf dereference the pointer of the [`Record`].
///
/// The advantage of using [`RecordResolver`] over using [`std::ops::Deref`] or [`std::ops::DerefMut`] directly is that
/// [`RecordResolver`] can bind the mutability of the [`Record`] with the owner of [`RecordResolver`], instead of the
/// [`RecordToken`].
///
/// It is useful for providing a safe API for the user when implementing a customized eviction algorithm with the
/// technique of intrusive data structure while letting foyer manage the lifetime of a [`Record`].
pub struct RecordResolver<K, V, H, S>(PhantomData<(K, V, H, S)>);

impl<K, V, H, S> RecordResolver<K, V, H, S> {
    pub(crate) fn new() -> Self {
        Self(PhantomData)
    }

    pub fn resolve(&self, token: RecordToken<K, V, H, S>) -> &Record<K, V, H, S> {
        unsafe { token.ptr.as_ref() }
    }

    pub fn resolve_mut(&mut self, mut token: RecordToken<K, V, H, S>) -> &mut Record<K, V, H, S> {
        unsafe { token.ptr.as_mut() }
    }
}

// #[derive(Default)]
// pub struct Link {
//     prev: Option<NonNull<()>>,
//     next: Option<NonNull<()>>,
// }

// pub struct DefaultRecordTokenList;

// pub trait RecordTokenListState<ID = DefaultRecordTokenList> {
//     fn link(&self) -> &Link;
//     fn link_mut(&mut self) -> &mut Link;
// }

// pub struct RecordTokenList<K, V, H, S, ID> {
//     head: Option<RecordToken<K, V, H, S>>,
//     tail: Option<RecordToken<K, V, H, S>>,

//     len: usize,

//     resolver: RecordResolver<K, V, H, S>,

//     _marker: PhantomData<ID>,
// }

// impl<K, V, H, S, ID> RecordTokenList<K, V, H, S, ID>
// where
//     S: RecordTokenListState<K, V, H, S, ID>,
// {
//     pub fn new() -> Self {
//         Self {
//             head: None,
//             tail: None,
//             len: 0,

//             resolver: RecordResolver::new(),

//             _marker: PhantomData,
//         }
//     }

//     // pub fn push_back(&mut self, token: RecordToken<K, V, H, S>) {}
// }

// pub struct RecordTokenListIter<'a, K, V, H, S, ID> {
//     token: Option<RecordToken<K, V, H, S>>,
//     list: &'a RecordTokenList<K, V, H, S, ID>,
// }

// pub struct RecordTokenListIterMut<'a, K, V, H, S, ID> {
//     token: Option<RecordToken<K, V, H, S>>,
//     list: &'a mut RecordTokenList<K, V, H, S, ID>,
// }

// impl<'a, K, V, H, S, ID> RecordTokenListIter<'a, K, V, H, S, ID>
// where
//     S: RecordTokenListState<K, V, H, S, ID>,
// {
//     /// Get the immutable reference in the current position.
//     pub fn record(&self) -> Option<&Record<K, V, H, S>> {
//         self.token.map(|token| self.list.resolver.resolve(token))
//     }

//     /// Move to next.
//     ///
//     /// If iter is on tail, move to null.
//     /// If iter is on null, move to head.
//     pub fn next(&mut self) {
//         self.token = match self.token {
//             Some(token) => self.link(token).next,
//             None => self.list.head,
//         }
//     }

//     /// Move to prev.
//     ///
//     /// If iter is on head, move to null.
//     /// If iter is on null, move to tail.
//     pub fn prev(&mut self) {
//         self.token = match self.token {
//             Some(token) => self.link(token).prev,
//             None => self.list.tail,
//         }
//     }

//     /// Move to front.
//     pub fn front(&mut self) {
//         self.token = self.list.head;
//     }

//     /// Move to back.
//     pub fn back(&mut self) {
//         self.token = self.list.tail;
//     }

//     /// Check if the iterator is in the first position of the intrusive double linked list.
//     pub fn is_head(&self) -> bool {
//         self.token == self.list.head
//     }

//     /// Check if the iterator is in the last position of the intrusive double linked list.
//     pub fn is_tail(&self) -> bool {
//         self.token == self.list.tail
//     }

//     fn link(&self, token: RecordToken<K, V, H, S>) -> &Link<K, V, H, S> {
//         self.list.resolver.resolve(token).state().link()
//     }
// }

// impl<'a, K, V, H, S, ID> RecordTokenListIterMut<'a, K, V, H, S, ID>
// where
//     S: RecordTokenListState<K, V, H, S, ID>,
// {
//     /// Get the immutable reference in the current position.
//     pub fn record(&self) -> Option<&Record<K, V, H, S>> {
//         self.token.map(|token| self.list.resolver.resolve(token))
//     }

//     /// Get the mutable reference in the current position.
//     pub fn record_mut(&mut self) -> Option<&mut Record<K, V, H, S>> {
//         self.token.map(|token| self.list.resolver.resolve_mut(token))
//     }

//     /// Move to next.
//     ///
//     /// If iter is on tail, move to null.
//     /// If iter is on null, move to head.
//     pub fn next(&mut self) {
//         self.token = match self.token {
//             Some(token) => self.link(token).next,
//             None => self.list.head,
//         }
//     }

//     /// Move to prev.
//     ///
//     /// If iter is on head, move to null.
//     /// If iter is on null, move to tail.
//     pub fn prev(&mut self) {
//         self.token = match self.token {
//             Some(token) => self.link(token).prev,
//             None => self.list.tail,
//         }
//     }

//     /// Move to front.
//     pub fn front(&mut self) {
//         self.token = self.list.head;
//     }

//     /// Move to back.
//     pub fn back(&mut self) {
//         self.token = self.list.tail;
//     }

//     /// Removes the current token from [`RecordTokenlist`] and move next.
//     pub fn remove(&mut self) -> Option<RecordToken<K, V, H, S>> {
//         let token = match self.token {
//             Some(token) => token,
//             None => return None,
//         };

//         let link = self.link(token);
//         let prev = link.prev;
//         let next = link.next;

//         // fix head and tail if node is either of that
//         if Some(token) == self.list.head {
//             self.list.head = next;
//         }
//         if Some(token) == self.list.tail {
//             self.list.tail = prev;
//         }

//         // fix the next and prev ptrs of the node before and after this
//         if let Some(prev) = prev {
//             self.link_mut(prev).next = next;
//         }
//         if let Some(next) = next {
//             self.link_mut(next).prev = prev;
//         }

//         let link = self.link_mut(token);
//         link.next = None;
//         link.prev = None;

//         self.list.len -= 1;

//         self.token = next;

//         Some(token)
//     }

//     /// Link a new token before the current one.
//     ///
//     /// If iter is on null, link to tail.
//     pub fn insert_before(&mut self, token_new: RecordToken<K, V, H, S>) {
//         match self.token {
//             Some(token) => self.link_before(token_new, token),
//             None => {
//                 self.link_between(token_new, self.list.tail, None);
//                 self.list.tail = Some(token_new);
//             }
//         }

//         if self.list.head == self.token {
//             self.list.head = Some(token_new);
//         }

//         self.list.len += 1;
//     }

//     /// Link a new token after the current one.
//     ///
//     /// If iter is on null, link to head.
//     pub fn insert_after(&mut self, token_new: RecordToken<K, V, H, S>) {
//         match self.token {
//             Some(token) => self.link_after(token_new, token),
//             None => {
//                 self.link_between(token_new, None, self.list.head);
//                 self.list.head = Some(token_new);
//             }
//         }

//         if self.list.tail == self.token {
//             self.list.tail = Some(token_new)
//         }

//         self.list.len += 1;
//     }

//     fn link_before(&mut self, link: RecordToken<K, V, H, S>, next: RecordToken<K, V, H, S>) {
//         self.link_between(link, self.link(next).prev, Some(next));
//     }

//     fn link_after(&mut self, link: RecordToken<K, V, H, S>, prev: RecordToken<K, V, H, S>) {
//         self.link_between(link, Some(prev), self.link(prev).next);
//     }

//     fn link_between(
//         &mut self,
//         token: RecordToken<K, V, H, S>,
//         prev: Option<RecordToken<K, V, H, S>>,
//         next: Option<RecordToken<K, V, H, S>>,
//     ) {
//         if let Some(prev) = prev {
//             self.link_mut(prev).next = Some(token);
//         }
//         if let Some(next) = next {
//             self.link_mut(next).prev = Some(token);
//         }
//         let link = self.link_mut(token);
//         link.prev = prev;
//         link.next = next;
//     }

//     /// Check if the iterator is in the first position of the intrusive double linked list.
//     pub fn is_head(&self) -> bool {
//         self.token == self.list.head
//     }

//     /// Check if the iterator is in the last position of the intrusive double linked list.
//     pub fn is_tail(&self) -> bool {
//         self.token == self.list.tail
//     }

//     fn link(&self, token: RecordToken<K, V, H, S>) -> &Link<K, V, H, S> {
//         self.list.resolver.resolve(token).state().link()
//     }

//     fn link_mut(&mut self, token: RecordToken<K, V, H, S>) -> &mut Link<K, V, H, S> {
//         self.list.resolver.resolve_mut(token).state_mut().link_mut()
//     }
// }

// impl<'a, K, V, H, S, ID> Iterator for RecordTokenListIter<'a, K, V, H, S, ID>
// where
//     S: RecordTokenListState<K, V, H, S, ID>,
// {
//     type Item = &'a Record<K, V, H, S>;

//     fn next(&mut self) -> Option<Self::Item> {
//         self.next();
//         self.token.map(|token| unsafe { token.ptr.as_ref() })
//     }
// }

// impl<'a, K, V, H, S, ID> Iterator for RecordTokenListIterMut<'a, K, V, H, S, ID>
// where
//     S: RecordTokenListState<K, V, H, S, ID>,
// {
//     type Item = &'a mut Record<K, V, H, S>;

//     fn next(&mut self) -> Option<Self::Item> {
//         self.next();
//         self.token.map(|mut token| unsafe { token.ptr.as_mut() })
//     }
// }

// #[cfg(test)]
// mod tests {

//     use itertools::Itertools;

//     use super::*;

//     struct SingleListState<K, V> {
//         link: Link<K, V, (), Self>,
//     }

//     // #[derive(Debug)]
//     // struct DlistItem {
//     //     link: DlistLink,
//     //     val: u64,
//     // }

//     // impl DlistItem {
//     //     fn new(val: u64) -> Self {
//     //         Self {
//     //             link: DlistLink::default(),
//     //             val,
//     //         }
//     //     }
//     // }

//     // #[derive(Debug, Default)]
//     // struct DlistAdapter;

//     // unsafe impl Adapter for DlistAdapter {
//     //     type Item = DlistItem;
//     //     type Link = DlistLink;

//     //     fn new() -> Self {
//     //         Self
//     //     }

//     //     unsafe fn link2ptr(&self, link: NonNull<Self::Link>) -> NonNull<Self::Item> {
//     //         NonNull::new_unchecked(crate::container_of!(link.as_ptr(), DlistItem, link))
//     //     }

//     //     unsafe fn ptr2link(&self, item: NonNull<Self::Item>) -> NonNull<Self::Link> {
//     //         NonNull::new_unchecked((item.as_ptr() as *const u8).add(std::mem::offset_of!(DlistItem, link)) as *mut _)
//     //     }
//     // }

//     // intrusive_adapter! { DlistArcAdapter = DlistItem { link: DlistLink } }

//     // #[test]
//     // fn test_dlist_simple() {
//     //     let mut l = Dlist::<DlistAdapter>::new();

//     //     l.push_back(unsafe { NonNull::new_unchecked(Box::into_raw(Box::new(DlistItem::new(2)))) });
//     //     l.push_front(unsafe { NonNull::new_unchecked(Box::into_raw(Box::new(DlistItem::new(1)))) });
//     //     l.push_back(unsafe { NonNull::new_unchecked(Box::into_raw(Box::new(DlistItem::new(3)))) });

//     //     let v = l.iter_mut().map(|item| item.val).collect_vec();
//     //     assert_eq!(v, vec![1, 2, 3]);
//     //     assert_eq!(l.len(), 3);

//     //     let mut iter = l.iter_mut();
//     //     iter.next();
//     //     iter.next();
//     //     assert_eq!(DlistIterMut::get(&iter).unwrap().val, 2);
//     //     let p2 = iter.remove();
//     //     let i2 = unsafe { Box::from_raw(p2.unwrap().as_ptr()) };
//     //     assert_eq!(i2.val, 2);
//     //     assert_eq!(DlistIterMut::get(&iter).unwrap().val, 3);
//     //     let v = l.iter_mut().map(|item| item.val).collect_vec();
//     //     assert_eq!(v, vec![1, 3]);
//     //     assert_eq!(l.len(), 2);

//     //     let p3 = l.pop_back();
//     //     let i3 = unsafe { Box::from_raw(p3.unwrap().as_ptr()) };
//     //     assert_eq!(i3.val, 3);
//     //     let p1 = l.pop_front();
//     //     let i1 = unsafe { Box::from_raw(p1.unwrap().as_ptr()) };
//     //     assert_eq!(i1.val, 1);
//     //     assert!(l.pop_front().is_none());
//     //     assert_eq!(l.len(), 0);
//     // }
// }
