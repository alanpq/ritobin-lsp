use std::borrow::Cow;

pub enum Hash<'a, T> {
    Hash(T),
    Unhash(Cow<'a, str>),
}
