//! An implementation of the Grapheme Cluster portion of
//! [UAX #29: Unicode Text Segmentation](https://www.unicode.org/reports/tr29/)
//! that prioritizes streaming-friendliness and simplicity.
//!
//! This library implements the segmentation algorithm as of Unicode 16.0.0,
//! using the character database tables from that release.
//!
//! [`GraphemeMachine`] is the main type in this library. Construct an object
//! of that type and then feed it characters from a stream one at a time, and
//! in return it will tell you for each new character whether it should be
//! treated as an extension of the current grapheme cluster or the beginning
//! of a new one. That's all there is to it!
//!
//! ---
//!
//! The canonical Rust library for UAX #29 is
//! [`unicode_segmentation`](https://docs.rs/unicode-segmentation/latest/unicode_segmentation/),
//! and so that's actually probably what you should use in most cases. This
//! library has the following main distinctions (as of
//! `unicode_segmentation` v1.12.0):
//!
//! - The primary entry point for grapheme clusters in `unicode_segmentation`
//!   is [`Graphemes`](https://docs.rs/unicode-segmentation/latest/unicode_segmentation/struct.Graphemes.html),
//!   which expects the entire text to be in memory as a single buffer.
//!
//!   The library also offers [`GraphemeCursor`](https://docs.rs/unicode-segmentation/latest/unicode_segmentation/struct.GraphemeCursor.html)
//!   for working with non-contiguous buffers, but it has a rather challenging
//!   API and is difficult to use in a _completely_ streaming manner, with
//!   the caller required to sometimes provide earlier context to help it make
//!   a decision.
//!
//!   By contrast, [`GraphemeMachine`] in this library is a finite state machine
//!   that is advanced one character at a time, with no requirement for the
//!   caller to do any buffering at all. Of course in practice it's likely that
//!   a normal caller will need to at least buffer the current grapheme cluster
//!   so it can be used once finally split, but how to manage that is left
//!   entirely up to the caller.
//!
//!   For example, a caller could decide that it only cares about grapheme
//!   clusters up to some reasonable maximum length, after which it will
//!   just assume malicious or corrupt input and use the Unicode replacement
//!   character instead. The [`GraphemeMachine`] can still allow that caller
//!   to find the end of that overlong grapheme cluster and begin consuming
//!   the next one even though the caller is no longer including any new
//!   characters into its buffer.
//!
//! - `unicode_segmentation` finds the relevant Unicode character properties
//!   for incoming characters using binary search over its internal tables,
//!   after converting the character into a Rust [`char`] value.
//!
//!   [`GraphemeMachine`] instead prefers to work with UTF-8 encoded characters
//!   as represented by [`u8char`], which can be more cheaply extracted from
//!   and appended to Rust strings. The character property lookup is done using
//!   a trie based on the UTF-8 byte sequence, and so is potentially faster
//!   when you're chomping UTF-8 sequences from a `str` buffer one at a time.
//!
//!   (That's not necessarily true, though. Measure it yourself with the text
//!   you want to segment if performance is important to you!)
//!
//! - Although [`GraphemeMachine`] can work with [`char`] and [`u8char`] values
//!   representing specific characters, the segmentation algorithm is actually
//!   defined in terms of groups of characters that share similar properties.
//!
//!   This library exposes those categories as part of its public API using
//!   [`CharProperties`], [`GCBProperty`], and [`InCBProperty`], and so it
//!   could be useful purely as a character property lookup library even
//!   if you don't use [`GraphemeMachine`], or you could even choose to use
//!   your own tailored character property tables and pass [`CharProperties`]
//!   values directly to a [`GraphemeMachine`] object.
//!
//! Unless you have a good reason to prefer this library though, it's probably
//! better to use
//! [`unicode_segmentation`](https://docs.rs/unicode-segmentation/latest/unicode_segmentation/)
//! because it's widely-used in the Rust community, well-maintained by an
//! established team (whereas _this_ library has only a single,
//! easily-distracted author), and probably not subject to the important caveat
//! described in the following section.
//!
//! # An important caveat
//!
//! The author originally wrote the code and lookup tables in this library
//! internally within another project, and then proceeded to copy it into
//! several other projects that needed grapheme cluster segmentation. This
//! library is the result of finally getting around to separating it out into
//! a separate unit for release.
//!
//! Unfortunately the code that generated the trie used for character property
//! lookup seems to be missing, and so this library will probably be tethered
//! to Unicode 16.0.0 indefinitely unless the author gets somehow inspired
//! to recreate that generation program. 😖 If staying up-to-date with new
//! Unicode versions is important to you then you should probably use
//! [`unicode_segmentation`](https://docs.rs/unicode-segmentation/latest/unicode_segmentation/)
//! instead.
//!
//! It would in principle be possible to use a property lookup table maintained
//! outside of this crate and then produce [`CharProperties`] values to pass
//! into a [`GraphemeMachine`] without using this library's lookup tables at
//! all, though I expect few would be motivated to do that.
#![cfg_attr(not(test), no_std)]

mod properties;
mod state;

use core::{iter::FusedIterator, marker::PhantomData};

pub use properties::*;

use state::State;
pub use u8char::u8char;

/// A finite state machine for detecting grapheme cluster boundaries.
///
/// This is a grapheme clustering implementation tailored for streaming input,
/// such as characters arriving over a network socket. It does not include
/// any text buffers of its own and doesn't require the caller to maintain any
/// buffers, although in practical applications the caller will presumably
/// want to keep _some_ sort of buffer of the characters from the current
/// grapheme cluster in progress.
///
/// As new characters arrive, feed them into the state machine sequentially
/// using [`Self::next_char_properties`], [`Self::next_u8char`], or
/// [`Self::next_char`], each of which will return an indicator for whether
/// that new character should be treated as the beginning of a new grapheme
/// cluster or as a continuation of the one already in progress.
///
/// Internally a `GraphemeMachine` tracks only the properties of the most
/// recently presented character (if any) and the current state from a finite
/// state machine that effectively encodes everything the segmentation algorithm
/// needs to know about all of the characters submitted so far into a single
/// byte. Each newly-submitted character therefore updates the record of
/// the most recent character and advances the internal state machine based
/// on the new character.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GraphemeMachine {
    state: State,
    prev: Option<CharProperties>,
}

impl GraphemeMachine {
    /// Constructs a new [`GraphemeMachine`] in an initial "start of input"
    /// state.
    pub const fn new() -> Self {
        GraphemeMachine {
            state: State::Base,
            prev: None,
        }
    }

    /// Advances the state machine for a character with the given properties,
    /// returning the action to take at the boundary between this and the
    /// previous character (if any).
    ///
    /// If the result is [`ClusterAction::Split`] then the character whose
    /// properties were submitted should be treated as the beginning of a new
    /// grapheme cluster. If [`ClusterAction::Continue`] then the new character
    /// should be treated as an extension of the current grapheme cluster.
    ///
    /// At the start of input when there is no previous character the action
    /// is always [`ClusterAction::Split`], because there is no current
    /// grapheme cluster to possibly extend.
    pub const fn next_char_properties(&mut self, next: CharProperties) -> ClusterAction {
        let (boundary, next_state) = self.state.transition(self.prev, next);
        self.state = next_state;
        self.prev = Some(next);
        if boundary {
            ClusterAction::Split
        } else {
            ClusterAction::Continue
        }
    }

    /// Looks up the [`CharProperties`] for the given character and then
    /// advances the state machine by passing it to [`Self::next_char_properties`].
    ///
    /// Refer to the documentation of that function for information on the
    /// meaning of the result.
    pub const fn next_u8char(&mut self, c: u8char) -> ClusterAction {
        let props = CharProperties::for_u8char(c);
        self.next_char_properties(props)
    }

    /// Looks up the [`CharProperties`] for the given character and then
    /// advances the state machine by passing it to [`Self::next_char_properties`].
    ///
    /// Refer to the documentation of that function for information on the
    /// meaning of the result.
    ///
    /// Note that this library's lookup table for [`CharProperties`] is optimized
    /// for fast lookup of [`u8char`] rather than [`char`], so this will
    /// first convert the given character to the `u8char` representation. If
    /// you already have the character in `u8char` form then you can avoid
    /// unnecessary conversions by calling [`Self::next_u8char`] instead.
    pub const fn next_char(&mut self, c: char) -> ClusterAction {
        let props = CharProperties::for_char(c);
        self.next_char_properties(props)
    }

    /// Returns an iterator which, on each call to [`Iterator::next`],
    /// takes another [`u8char`] from the prefix of `s`, feeds it into
    /// the state machine using [`Self::next_u8char`], and then returns
    /// the indicated [`ClusterAction`] along with the character that
    /// caused it.
    ///
    /// If you don't keep reading the iterator until it returns `None`
    /// then the state machine will only have dealt with the items
    /// previously returned, and so you'd need to have been counting
    /// how many UTF-8 bytes' worth of characters you'd consumed so
    /// far in order to know how much of the string had been processed.
    /// Always consuming the entire iterator makes things easier to
    /// keep track of.
    ///
    /// There is no automatic call to [`Self::end_of_input`] once the
    /// end of the string is reached, so it's okay to provide streaming
    /// input in a series of [`str`] chunks even if there are grapheme
    /// clusters straddling across the buffer boundaries.
    pub const fn next_u8chars_from_str<'a>(&'a mut self, s: &'a str) -> IterChar<'a, u8char> {
        IterChar::<u8char> {
            machine: self,
            remain: s,
            _marker: PhantomData,
        }
    }

    /// Behaves the same as [`Self::next_u8chars_from_str`] except that it
    /// also converts the characters to [`char`], for more convenient use
    /// by callers who are interacting with something that only supports
    /// Rust's standard character representation.
    pub const fn next_chars_from_str<'a>(&'a mut self, s: &'a str) -> IterChar<'a, char> {
        IterChar::<char> {
            machine: self,
            remain: s,
            _marker: PhantomData,
        }
    }

    /// Tells the state machine that the input stream has ended.
    ///
    /// This resets the state machine to the "start of input" state so that
    /// any subsequently-submitted character cannot be treated as a continuation
    /// of the current grapheme cluster.
    ///
    /// This is named "end of input" because that's the terminology used in
    /// the Unicode text segmentation spec, but this could be used for any
    /// situation where the caller knows there is some non-text-related
    /// boundary between characters in a stream, such as when parsing a markup
    /// language and encountering the start of a tag or delimiter instead of
    /// literal text. In that case it's typically expected that whatever literal
    /// character follows the tag is treated as the beginning of a new grapheme
    /// cluster, regardless of what came before the tag.
    ///
    /// For consistency with the other machine-advancing methods this returns
    /// an action to take, but at the end of input the action is always
    /// [`ClusterAction::Split`] to mark the end of the final grapheme cluster.
    pub const fn end_of_input(&mut self) -> ClusterAction {
        self.state = State::Base;
        self.prev = None;
        ClusterAction::Split
    }

    /// Const-compatible `Eq`.
    pub const fn eq(self, other: Self) -> bool {
        self.state.eq(other.state)
            && match (self.prev, other.prev) {
                (Some(prev), Some(other_prev)) => prev.eq(other_prev),
                (None, None) => true,
                _ => false,
            }
    }
}

/// What to do with a new character after presenting it to a [GraphemeMachine].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClusterAction {
    /// Treat the new character as an extension of the current grapheme cluster.
    Continue,
    /// Treat the current grapheme cluster as complete and begin a new one
    /// that initially consists only of the new character.
    Split,
}
impl ClusterAction {
    /// Const-compatible `Eq`.
    pub const fn eq(self, other: Self) -> bool {
        matches!(
            (self, other),
            (Self::Continue, Self::Continue) | (Self::Split, Self::Split)
        )
    }
}

/// An iterator over characters of type either u8char or char.
#[doc(hidden)]
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct IterChar<'a, T> {
    machine: &'a mut GraphemeMachine,
    remain: &'a str,
    _marker: PhantomData<T>,
}
impl<'a> IterChar<'a, u8char> {
    pub const fn next(&mut self) -> Option<(ClusterAction, u8char)> {
        let (next, rest) = u8char::from_string_prefix(self.remain);
        let Some(next) = next else {
            return None;
        };
        let action = self.machine.next_u8char(next);
        self.remain = rest;
        Some((action, next))
    }
}
impl<'a> Iterator for IterChar<'a, u8char> {
    type Item = (ClusterAction, u8char);
    fn next(&mut self) -> Option<Self::Item> {
        self.next()
    }
}
impl<'a> FusedIterator for IterChar<'a, u8char> {}

impl<'a> IterChar<'a, char> {
    pub const fn next(&mut self) -> Option<(ClusterAction, char)> {
        let (next, rest) = u8char::from_string_prefix(self.remain);
        let Some(next) = next else {
            return None;
        };
        let action = self.machine.next_u8char(next);
        self.remain = rest;
        Some((action, next.to_char()))
    }
}
impl<'a> Iterator for IterChar<'a, char> {
    type Item = (ClusterAction, char);
    fn next(&mut self) -> Option<Self::Item> {
        self.next()
    }
}
impl<'a> FusedIterator for IterChar<'a, char> {}

#[cfg(test)]
mod tests;
