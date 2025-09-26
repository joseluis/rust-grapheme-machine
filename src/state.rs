use crate::CharProperties;
use crate::GCBProperty;
use crate::InCBProperty;

/// State represents different states we can transition through while detecting
/// grapheme cluster boundaries.
///
/// A [`State`] value essentially summarizes a set of category transitions that
/// happened before the current one, so that we can detect arbitrary-long
/// grapheme clusters using only finite storage.
///
/// (In order to actually _use_ a detected grapheme cluster after its bounds
/// have been found would require the caller to have buffered everything that
/// appeared since the last boundary, but the RISCovite VT system isn't capable
/// of advanced text shaping anyway, so clusters over a certain length cannot be
/// rendered anyway and so in that case we just want to find the beginning of
/// the next cluster so we can know when to stop discarding overlong input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// The initial state at the beginning of the text or when the following
    /// should be treated as if it were at the beginning of the text.
    Base,

    /// When the previous character was [`GcCategory::RegionalIndicator`]
    /// but its predecessor was not, and therefore if the next character
    /// is also [`GcCategory::RegionalIndicator`] the two together represent
    /// an emoji flag under rules GB12 and GB13.
    AwaitEmojiFlag,

    /// When consecutive characters before `prev` matched
    /// `\p{Extended_Pictographic} Extend*`, and so if the next character
    /// is [`GcCategory::ZWJ`] we should transition to [`State::GB11AfterZWJ`].
    GB11BeforeZWJ,

    /// The [`GcCategory::ZWJ`] currently in `prev` arrived while in
    /// [`State::GB11BeforeZWJ`], and so rule GB11 is active.
    GB11AfterZWJ,

    /// We've encountered `\p{InCB=Consonant}` followed by zero or more
    /// `\p{InCB=Extend}`, with no `\p{InCB=Linker}` in between.
    GB9cConsonant,
    /// We've encountered `\p{InCB=Consonant}` followed by a least one
    /// `\p{InCB=Linker}` and zero or more `\p{InCB=Extend}` (in any order).
    GB9cLinker,
}

impl State {
    /// Given the previous category and the next category, returns whether
    /// there is a grapheme cluster boundary between two characters of those
    /// categories in the current state, and the state that should be used for
    /// the next transition.
    ///
    /// Use [`GcProperties::Other`] to represent the absense of a character at
    /// the start or end of input.
    ///
    /// Correct use requires that the `prev` of one call equals the `next`
    /// of the previous call that generated the new state. If that is not
    /// upheld then the results are unspecified.
    pub const fn transition(
        self,
        prev: Option<CharProperties>,
        next: CharProperties,
    ) -> (bool, State) {
        use GCBProperty::*;

        let next_state = self.next_state(next);
        let Some(prev) = prev else {
            // At start of input there's always a boundary.
            return (true, next_state);
        };

        // GB1 and GB2 aren't covered here because we use "Other" to represent
        // the beginning and end of the data, which will therefore always
        // fall through to the final last-resort `return` below.

        macro_rules! pair_matches {
            ($prev:pat, $next:pat) => {
                matches!(prev.gcb_property(), $prev) && matches!(next.gcb_property(), $next)
            };
        }
        macro_rules! one_matches {
            ($which:expr, $pat:pat) => {
                matches!($which.gcb_property(), $pat)
            };
        }

        // GB3: Do not break between a CR and LF...
        if pair_matches!(CR, LF) {
            return (false, next_state);
        }
        // GB4 and GB5: ...Otherwise, break before and after controls.
        if prev.is_any_control() || next.is_any_control() {
            return (true, next_state);
        }
        // GB6: Do not break Hangul syllable or other conjoining sequences.
        if pair_matches!(L, L | V | LV | LVT) {
            return (false, next_state);
        }
        // GB7: Do not break Hangul syllable or other conjoining sequences.
        if pair_matches!(LV | V, V | T) {
            return (false, next_state);
        }
        // GB8: Do not break Hangul syllable or other conjoining sequences.
        if pair_matches!(LVT | T, T) {
            return (false, next_state);
        }
        // GB9: Do not break before extending characters or ZWJ.
        if one_matches!(next, Extend | ZWJ) {
            return (false, next_state);
        }
        // GB9a: Do not break before SpacingMarks...
        if one_matches!(next, SpacingMark) {
            return (false, next_state);
        }
        // GB9b: ...or after Prepend characters.
        if one_matches!(prev, Prepend) {
            return (false, next_state);
        }
        // GB9c: Do not break within certain combinations with Indic_Conjunct_Break (InCB)=Linker
        if self.gb9c_active() {
            if matches!(
                prev.incb_property(),
                InCBProperty::Linker | InCBProperty::Extend
            ) && matches!(next.incb_property(), InCBProperty::Consonant)
            {
                return (false, next_state);
            }
        }
        // (GB10 was from an earlier version of the specification but is no longer used)
        // GB11: Do not break within emoji modifier sequences or emoji zwj sequences.
        if self.gb11_active() {
            if pair_matches!(ZWJ, ExtendedPictographic) {
                return (false, next_state);
            }
        }
        // GB12 and GB13: Do not break within emoji flag sequences.
        if self.gb13_active() {
            if pair_matches!(RegionalIndicator, RegionalIndicator) {
                return (false, next_state);
            }
        }

        // GB999: Otherwise, break everywhere.
        return (true, next_state);
    }

    /// Returns the next state that the state machine transitions to when
    /// encountering the given character.
    const fn next_state(self, next: CharProperties) -> Self {
        use GCBProperty::*;
        use State::*;
        // Two of the multi-character prefixes can begin regardless of
        // what preceeds them. These don't need to be covered by the
        // state-specific arms that fllow.
        if matches!(next.gcb_property(), ExtendedPictographic) {
            return GB11BeforeZWJ;
        }
        if matches!(next.incb_property(), InCBProperty::Consonant) {
            return GB9cConsonant;
        }
        let gc_prop = next.gcb_property();
        let incb_prop = next.incb_property();
        match self {
            Base => match gc_prop {
                RegionalIndicator => AwaitEmojiFlag,
                _ => Base,
            },
            AwaitEmojiFlag => Base,
            GB11BeforeZWJ => match gc_prop {
                ZWJ => GB11AfterZWJ,
                Extend => GB11BeforeZWJ,
                _ => Base,
            },
            GB11AfterZWJ => Base,
            GB9cConsonant => match incb_prop {
                InCBProperty::Linker => GB9cLinker,
                InCBProperty::Extend => GB9cConsonant,
                _ => Base,
            },
            GB9cLinker => match incb_prop {
                InCBProperty::Linker | InCBProperty::Extend => GB9cLinker,
                _ => Base,
            },
        }
    }

    const fn gb9c_active(self) -> bool {
        // GB9c only active in GB9cConsonantExtendLinkerLinker state
        matches!(self, Self::GB9cLinker)
    }

    const fn gb11_active(self) -> bool {
        // GB11 only active in GB11AfterZWJ state
        matches!(self, Self::GB11AfterZWJ)
    }

    const fn gb13_active(self) -> bool {
        // GB12/GB13 only active in AwaitEmojiFlag state
        matches!(self, Self::AwaitEmojiFlag)
    }
}

#[cfg(test)]
mod tests;
