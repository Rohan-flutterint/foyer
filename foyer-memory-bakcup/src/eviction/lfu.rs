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

use std::{fmt::Debug, ptr::NonNull};

use cmsketch::CMSketchU16;
use foyer_common::{assert::OptionExt, strict_assert, strict_assert_eq, strict_assert_ne};
use foyer_intrusive::{
    adapter::Link,
    dlist::{Dlist, DlistLink},
    intrusive_adapter,
};
use serde::{Deserialize, Serialize};

use crate::{
    eviction::Eviction,
    handle::{BaseHandle, Handle},
    CacheContext,
};

/// w-TinyLFU eviction algorithm config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LfuConfig {
    /// `window` capacity ratio of the total cache capacity.
    ///
    /// Must be in (0, 1).
    ///
    /// Must guarantee `window_capacity_ratio + protected_capacity_ratio < 1`.
    pub window_capacity_ratio: f64,
    /// `protected` capacity ratio of the total cache capacity.
    ///
    /// Must be in (0, 1).
    ///
    /// Must guarantee `window_capacity_ratio + protected_capacity_ratio < 1`.
    pub protected_capacity_ratio: f64,

    /// Error of the count-min sketch.
    ///
    /// See [`CMSketchU16::new`].
    pub cmsketch_eps: f64,

    /// Confidence of the count-min sketch.
    ///
    /// See [`CMSketchU16::new`].
    pub cmsketch_confidence: f64,
}

impl Default for LfuConfig {
    fn default() -> Self {
        Self {
            window_capacity_ratio: 0.1,
            protected_capacity_ratio: 0.8,
            cmsketch_eps: 0.001,
            cmsketch_confidence: 0.9,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LfuContext(CacheContext);

impl From<CacheContext> for LfuContext {
    fn from(context: CacheContext) -> Self {
        Self(context)
    }
}

impl From<LfuContext> for CacheContext {
    fn from(context: LfuContext) -> Self {
        context.0
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Queue {
    None,
    Window,
    Probation,
    Protected,
}

pub struct LfuHandle<T>
where
    T: Send + Sync + 'static,
{
    link: DlistLink,
    base: BaseHandle<T, LfuContext>,
    queue: Queue,
}

impl<T> Debug for LfuHandle<T>
where
    T: Send + Sync + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LfuHandle").finish()
    }
}

intrusive_adapter! { LfuHandleDlistAdapter<T> = LfuHandle<T> { link: DlistLink } where T: Send + Sync + 'static }

impl<T> Default for LfuHandle<T>
where
    T: Send + Sync + 'static,
{
    fn default() -> Self {
        Self {
            link: DlistLink::default(),
            base: BaseHandle::new(),
            queue: Queue::None,
        }
    }
}

impl<T> Handle for LfuHandle<T>
where
    T: Send + Sync + 'static,
{
    type Data = T;
    type Context = LfuContext;

    fn base(&self) -> &BaseHandle<Self::Data, Self::Context> {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseHandle<Self::Data, Self::Context> {
        &mut self.base
    }
}

unsafe impl<T> Send for LfuHandle<T> where T: Send + Sync + 'static {}
unsafe impl<T> Sync for LfuHandle<T> where T: Send + Sync + 'static {}

/// This implementation is inspired by [Caffeine](https://github.com/ben-manes/caffeine) under Apache License 2.0
///
/// A new and hot entry is kept in `window`.
///
/// When `window` is full, entries from it will overflow to `probation`.
///
/// When a entry in `probation` is accessed, it will be promoted to `protected`.
///
/// When `protected` is full, entries from it will overflow to `probation`.
///
/// When evicting, the entry with a lower frequency from `window` or `probation` will be evicted first, then from
/// `protected`.
pub struct Lfu<T>
where
    T: Send + Sync + 'static,
{
    window: Dlist<LfuHandleDlistAdapter<T>>,
    probation: Dlist<LfuHandleDlistAdapter<T>>,
    protected: Dlist<LfuHandleDlistAdapter<T>>,

    window_weight: usize,
    probation_weight: usize,
    protected_weight: usize,

    window_weight_capacity: usize,
    protected_weight_capacity: usize,

    frequencies: CMSketchU16,

    step: usize,
    decay: usize,
}

impl<T> Lfu<T>
where
    T: Send + Sync + 'static,
{
    fn increase_queue_weight(&mut self, handle: &LfuHandle<T>) {
        let weight = handle.base().weight();
        match handle.queue {
            Queue::None => unreachable!(),
            Queue::Window => self.window_weight += weight,
            Queue::Probation => self.probation_weight += weight,
            Queue::Protected => self.protected_weight += weight,
        }
    }

    fn decrease_queue_weight(&mut self, handle: &LfuHandle<T>) {
        let weight = handle.base().weight();
        match handle.queue {
            Queue::None => unreachable!(),
            Queue::Window => self.window_weight -= weight,
            Queue::Probation => self.probation_weight -= weight,
            Queue::Protected => self.protected_weight -= weight,
        }
    }

    fn update_frequencies(&mut self, hash: u64) {
        self.frequencies.inc(hash);
        self.step += 1;
        if self.step >= self.decay {
            self.step >>= 1;
            self.frequencies.halve();
        }
    }
}

impl<T> Eviction for Lfu<T>
where
    T: Send + Sync + 'static,
{
    type Handle = LfuHandle<T>;
    type Config = LfuConfig;

    unsafe fn new(capacity: usize, config: &Self::Config) -> Self
    where
        Self: Sized,
    {
        assert!(
            config.window_capacity_ratio > 0.0 && config.window_capacity_ratio < 1.0,
            "window_capacity_ratio must be in (0, 1), given: {}",
            config.window_capacity_ratio
        );

        assert!(
            config.protected_capacity_ratio > 0.0 && config.protected_capacity_ratio < 1.0,
            "protected_capacity_ratio must be in (0, 1), given: {}",
            config.protected_capacity_ratio
        );

        assert!(
            config.window_capacity_ratio + config.protected_capacity_ratio < 1.0,
            "must guarantee: window_capacity_ratio + protected_capacity_ratio < 1, given: {}",
            config.window_capacity_ratio + config.protected_capacity_ratio
        );

        let window_weight_capacity = (capacity as f64 * config.window_capacity_ratio) as usize;
        let protected_weight_capacity = (capacity as f64 * config.protected_capacity_ratio) as usize;
        let frequencies = CMSketchU16::new(config.cmsketch_eps, config.cmsketch_confidence);
        let decay = frequencies.width();

        Self {
            window: Dlist::new(),
            probation: Dlist::new(),
            protected: Dlist::new(),
            window_weight: 0,
            probation_weight: 0,
            protected_weight: 0,
            window_weight_capacity,
            protected_weight_capacity,
            frequencies,
            step: 0,
            decay,
        }
    }

    unsafe fn push(&mut self, mut ptr: NonNull<Self::Handle>) {
        let handle = ptr.as_mut();

        strict_assert!(!handle.link.is_linked());
        strict_assert!(!handle.base().is_in_eviction());
        strict_assert_eq!(handle.queue, Queue::None);

        self.window.push_back(ptr);
        handle.base_mut().set_in_eviction(true);
        handle.queue = Queue::Window;

        self.increase_queue_weight(handle);
        self.update_frequencies(handle.base().hash());

        // If `window` weight exceeds the capacity, overflow entry from `window` to `probation`.
        while self.window_weight > self.window_weight_capacity {
            strict_assert!(!self.window.is_empty());
            let mut ptr = self.window.pop_front().strict_unwrap_unchecked();
            let handle = ptr.as_mut();
            self.decrease_queue_weight(handle);
            handle.queue = Queue::Probation;
            self.increase_queue_weight(handle);
            self.probation.push_back(ptr);
        }
    }

    unsafe fn pop(&mut self) -> Option<NonNull<Self::Handle>> {
        // Compare the frequency of the front element of `window` and `probation` queue, and evict the lower one.
        // If both `window` and `probation` are empty, try evict from `protected`.
        let mut ptr = match (self.window.front(), self.probation.front()) {
            (None, None) => None,
            (None, Some(_)) => self.probation.pop_front(),
            (Some(_), None) => self.window.pop_front(),
            (Some(window), Some(probation)) => {
                if self.frequencies.estimate(window.base().hash()) < self.frequencies.estimate(probation.base().hash())
                {
                    self.window.pop_front()

                    // TODO(MrCroxx): Rotate probation to prevent a high frequency but cold head holds back promotion
                    // too long like CacheLib does?
                } else {
                    self.probation.pop_front()
                }
            }
        }
        .or_else(|| self.protected.pop_front())?;

        let handle = ptr.as_mut();

        strict_assert!(!handle.link.is_linked());
        strict_assert!(handle.base().is_in_eviction());
        strict_assert_ne!(handle.queue, Queue::None);

        self.decrease_queue_weight(handle);
        handle.queue = Queue::None;
        handle.base_mut().set_in_eviction(false);

        Some(ptr)
    }

    unsafe fn release(&mut self, mut ptr: NonNull<Self::Handle>) {
        let handle = ptr.as_mut();

        match handle.queue {
            Queue::None => {
                strict_assert!(!handle.link.is_linked());
                strict_assert!(!handle.base().is_in_eviction());
                self.push(ptr);
                strict_assert!(handle.link.is_linked());
                strict_assert!(handle.base().is_in_eviction());
            }
            Queue::Window => {
                // Move to MRU position of `window`.
                strict_assert!(handle.link.is_linked());
                strict_assert!(handle.base().is_in_eviction());
                self.window.remove_raw(handle.link.raw());
                self.window.push_back(ptr);
            }
            Queue::Probation => {
                // Promote to MRU position of `protected`.
                strict_assert!(handle.link.is_linked());
                strict_assert!(handle.base().is_in_eviction());
                self.probation.remove_raw(handle.link.raw());
                self.decrease_queue_weight(handle);
                handle.queue = Queue::Protected;
                self.increase_queue_weight(handle);
                self.protected.push_back(ptr);

                // If `protected` weight exceeds the capacity, overflow entry from `protected` to `probation`.
                while self.protected_weight > self.protected_weight_capacity {
                    strict_assert!(!self.protected.is_empty());
                    let mut ptr = self.protected.pop_front().strict_unwrap_unchecked();
                    let handle = ptr.as_mut();
                    self.decrease_queue_weight(handle);
                    handle.queue = Queue::Probation;
                    self.increase_queue_weight(handle);
                    self.probation.push_back(ptr);
                }
            }
            Queue::Protected => {
                // Move to MRU position of `protected`.
                strict_assert!(handle.link.is_linked());
                strict_assert!(handle.base().is_in_eviction());
                self.protected.remove_raw(handle.link.raw());
                self.protected.push_back(ptr);
            }
        }
    }

    unsafe fn acquire(&mut self, ptr: NonNull<Self::Handle>) {
        self.update_frequencies(ptr.as_ref().base().hash());
    }

    unsafe fn remove(&mut self, mut ptr: NonNull<Self::Handle>) {
        let handle = ptr.as_mut();

        strict_assert!(handle.link.is_linked());
        strict_assert!(handle.base().is_in_eviction());
        strict_assert_ne!(handle.queue, Queue::None);

        match handle.queue {
            Queue::None => unreachable!(),
            Queue::Window => self.window.remove_raw(handle.link.raw()),
            Queue::Probation => self.probation.remove_raw(handle.link.raw()),
            Queue::Protected => self.protected.remove_raw(handle.link.raw()),
        };

        strict_assert!(!handle.link.is_linked());

        self.decrease_queue_weight(handle);
        handle.queue = Queue::None;
        handle.base_mut().set_in_eviction(false);
    }

    unsafe fn clear(&mut self) -> Vec<NonNull<Self::Handle>> {
        let mut res = Vec::with_capacity(self.len());

        while !self.is_empty() {
            let ptr = self.pop().strict_unwrap_unchecked();
            strict_assert!(!ptr.as_ref().base().is_in_eviction());
            strict_assert!(!ptr.as_ref().link.is_linked());
            strict_assert_eq!(ptr.as_ref().queue, Queue::None);
            res.push(ptr);
        }

        res
    }

    fn len(&self) -> usize {
        self.window.len() + self.probation.len() + self.protected.len()
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

unsafe impl<T> Send for Lfu<T> where T: Send + Sync + 'static {}
unsafe impl<T> Sync for Lfu<T> where T: Send + Sync + 'static {}

#[cfg(test)]
mod tests {

    use itertools::Itertools;

    use super::*;
    use crate::{eviction::test_utils::TestEviction, handle::HandleExt};

    impl<T> TestEviction for Lfu<T>
    where
        T: Send + Sync + 'static + Clone,
    {
        fn dump(&self) -> Vec<T> {
            self.window
                .iter()
                .chain(self.probation.iter())
                .chain(self.protected.iter())
                .map(|handle| handle.base().data_unwrap_unchecked().clone())
                .collect_vec()
        }
    }

    type TestLfu = Lfu<u64>;
    type TestLfuHandle = LfuHandle<u64>;

    unsafe fn assert_test_lfu(
        lfu: &TestLfu,
        len: usize,
        window: usize,
        probation: usize,
        protected: usize,
        entries: Vec<u64>,
    ) {
        assert_eq!(lfu.len(), len);
        assert_eq!(lfu.window.len(), window);
        assert_eq!(lfu.probation.len(), probation);
        assert_eq!(lfu.protected.len(), protected);
        assert_eq!(lfu.window_weight, window);
        assert_eq!(lfu.probation_weight, probation);
        assert_eq!(lfu.protected_weight, protected);
        let es = lfu.dump().into_iter().collect_vec();
        assert_eq!(es, entries);
    }

    fn assert_min_frequency(lfu: &TestLfu, hash: u64, count: usize) {
        let freq = lfu.frequencies.estimate(hash);
        assert!(freq >= count as u16, "assert {freq} >= {count} failed for {hash}");
    }

    #[test]
    fn test_lfu() {
        unsafe {
            let ptrs = (0..100)
                .map(|i| {
                    let mut handle = Box::<TestLfuHandle>::default();
                    handle.init(i, i, 1, LfuContext(CacheContext::Default));
                    NonNull::new_unchecked(Box::into_raw(handle))
                })
                .collect_vec();

            // window: 2, probation: 2, protected: 6
            let config = LfuConfig {
                window_capacity_ratio: 0.2,
                protected_capacity_ratio: 0.6,
                cmsketch_eps: 0.01,
                cmsketch_confidence: 0.95,
            };
            let mut lfu = TestLfu::new(10, &config);

            assert_eq!(lfu.window_weight_capacity, 2);
            assert_eq!(lfu.protected_weight_capacity, 6);

            lfu.push(ptrs[0]);
            lfu.push(ptrs[1]);
            assert_test_lfu(&lfu, 2, 2, 0, 0, vec![0, 1]);

            lfu.push(ptrs[2]);
            lfu.push(ptrs[3]);
            assert_test_lfu(&lfu, 4, 2, 2, 0, vec![2, 3, 0, 1]);

            (4..10).for_each(|i| lfu.push(ptrs[i]));
            assert_test_lfu(&lfu, 10, 2, 8, 0, vec![8, 9, 0, 1, 2, 3, 4, 5, 6, 7]);

            (0..10).for_each(|i| assert_min_frequency(&lfu, i, 1));

            // [8, 9] [1, 2, 3, 4, 5, 6, 7]
            let p0 = lfu.pop().unwrap();
            assert_eq!(p0, ptrs[0]);

            // [9, 0] [1, 2, 3, 4, 5, 6, 7, 8]
            lfu.release(p0);
            assert_test_lfu(&lfu, 10, 2, 8, 0, vec![9, 0, 1, 2, 3, 4, 5, 6, 7, 8]);

            // [0, 9] [1, 2, 3, 4, 5, 6, 7, 8]
            lfu.release(ptrs[9]);
            assert_test_lfu(&lfu, 10, 2, 8, 0, vec![0, 9, 1, 2, 3, 4, 5, 6, 7, 8]);

            // [0, 9] [1, 2, 7, 8] [3, 4, 5, 6]
            (3..7).for_each(|i| lfu.release(ptrs[i]));
            assert_test_lfu(&lfu, 10, 2, 4, 4, vec![0, 9, 1, 2, 7, 8, 3, 4, 5, 6]);

            // [0, 9] [1, 2, 7, 8] [5, 6, 3, 4]
            (3..5).for_each(|i| lfu.release(ptrs[i]));
            assert_test_lfu(&lfu, 10, 2, 4, 4, vec![0, 9, 1, 2, 7, 8, 5, 6, 3, 4]);

            // [0, 9] [5, 6] [3, 4, 1, 2, 7, 8]
            [1, 2, 7, 8].into_iter().for_each(|i| lfu.release(ptrs[i]));
            assert_test_lfu(&lfu, 10, 2, 2, 6, vec![0, 9, 5, 6, 3, 4, 1, 2, 7, 8]);

            // [0, 9] [6] [3, 4, 1, 2, 7, 8]
            let p5 = lfu.pop().unwrap();
            assert_eq!(p5, ptrs[5]);
            assert_test_lfu(&lfu, 9, 2, 1, 6, vec![0, 9, 6, 3, 4, 1, 2, 7, 8]);

            (10..13).for_each(|i| lfu.push(ptrs[i]));

            // [11, 12] [6, 0, 9, 10] [3, 4, 1, 2, 7, 8]
            assert_test_lfu(&lfu, 12, 2, 4, 6, vec![11, 12, 6, 0, 9, 10, 3, 4, 1, 2, 7, 8]);
            (1..13).for_each(|i| assert_min_frequency(&lfu, i, 0));
            lfu.acquire(ptrs[0]);
            assert_min_frequency(&lfu, 0, 2);

            // evict 11 because freq(11) < freq(0)
            // [12] [0, 9, 10] [3, 4, 1, 2, 7, 8]
            let p6 = lfu.pop().unwrap();
            let p11 = lfu.pop().unwrap();
            assert_eq!(p6, ptrs[6]);
            assert_eq!(p11, ptrs[11]);
            assert_test_lfu(&lfu, 10, 1, 3, 6, vec![12, 0, 9, 10, 3, 4, 1, 2, 7, 8]);

            assert_eq!(
                lfu.clear(),
                [12, 0, 9, 10, 3, 4, 1, 2, 7, 8]
                    .into_iter()
                    .map(|i| ptrs[i])
                    .collect_vec()
            );

            for ptr in ptrs {
                let _ = Box::from_raw(ptr.as_ptr());
            }
        }
    }
}
