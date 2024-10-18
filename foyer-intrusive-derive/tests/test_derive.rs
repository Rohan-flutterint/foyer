use foyer_intrusive_derive::IntrusiveList;
use foyer_intrusive_v2::list::link;

pub struct Record<K, V> {
    key: K,
    value: V,
    state: State<K, V>,
}

#[derive(IntrusiveList)]
#[item(Record<K, V>)]
pub struct State<K, V> {
    key: K,
    value: V,
    val: u64,
    #[linker]
    link1: link,
    #[linker]
    link2: link,
}
