use u8char::u8char;

mod table;

/// Enumeration of **Grapheme_Cluster_Break** property values, from
/// [UAX#29 Section 3.1](https://www.unicode.org/reports/tr29/#Grapheme_Cluster_Break_Property_Values).
///
/// The ExtendedPictorgraphic is actually derived from the Emoji standard's
/// character tables, but is treated by UAX#29 as mutually-exclusive with the
/// grapheme cluster break property value and so included in this enumeration
/// for simplicity's sake.
///
/// Do not depend on the specific values currently used in this enumeration;
/// they are an implementation detail subject to change in future versions of
/// this library.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GCBProperty {
    /// Represents that none of the grapheme cluster break property values
    /// apply to a particular character at all.
    None = 0x00,
    CR = 0x01,
    Control = 0x02,
    Extend = 0x03,
    ExtendedPictographic = 0x04,
    L = 0x05,
    LF = 0x06,
    LV = 0x07,
    LVT = 0x08,
    Prepend = 0x09,
    RegionalIndicator = 0x0a,
    SpacingMark = 0x0b,
    T = 0x0c,
    V = 0x0d,
    ZWJ = 0x0e,
}
impl GCBProperty {
    /// Const-compatible `Eq`.
    pub const fn eq(self, other: Self) -> bool {
        matches!(
            (self, other),
            (Self::None, Self::None)
                | (Self::CR, Self::CR)
                | (Self::Control, Self::Control)
                | (Self::Extend, Self::Extend)
                | (Self::ExtendedPictographic, Self::ExtendedPictographic)
                | (Self::L, Self::L)
                | (Self::LF, Self::LF)
                | (Self::LV, Self::LV)
                | (Self::LVT, Self::LVT)
                | (Self::Prepend, Self::Prepend)
                | (Self::RegionalIndicator, Self::RegionalIndicator)
                | (Self::SpacingMark, Self::SpacingMark)
                | (Self::T, Self::T)
                | (Self::V, Self::V)
                | (Self::ZWJ, Self::ZWJ)
        )
    }
}

/// Enumeration of **Indic_Conjunct_Break** property values, as defined in
/// DerivedCoreProperties.txt based on
/// [the rules in UAX#44](https://www.unicode.org/reports/tr44/#Indic_Conjunct_Break).
///
/// These are used in the rule that avoids splitting orthographic syllables in
/// inappropriate ways, [GB9c](https://www.unicode.org/reports/tr29/#GB9c).
///
/// Do not depend on the specific values currently used in this enumeration;
/// they are an implementation detail subject to change in future versions of
/// this library.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InCBProperty {
    /// Represents that none of the Indic_Conjunct_Break property values
    /// apply to a particular character at all.
    None = 0x00,
    Consonant = 0x10,
    Extend = 0x20,
    Linker = 0x30,
}
impl InCBProperty {
    /// Const-compatible `Eq`.
    pub const fn eq(self, other: Self) -> bool {
        matches!(
            (self, other),
            (Self::None, Self::None)
                | (Self::Consonant, Self::Consonant)
                | (Self::Extend, Self::Extend)
                | (Self::Linker, Self::Linker)
        )
    }
}

/// Represents selections from the two derived Unicode character properties
/// used for grapheme cluster segmenttion:
///
/// - [`GCBProperty`] representing **Grapheme_Cluster_Break** property values.
/// - [`InCBProperty`] representing **Indic_Conjunct_Break** property values.
///
/// The [Grapheme Cluster Boundary Rules](https://www.unicode.org/reports/tr29/#Grapheme_Cluster_Boundary_Rules)
/// are defined in terms of both sets of property values, and so this type
/// serves as a compact tuple of one selection from each.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CharProperties {
    /// Bitfield representation of the property tuple. The enum values
    /// of [`GCBProperty`] and [`InCBProperty`] are defined such that one
    /// of each can be bitwise-ORed together without collisions, with
    /// the former in the low nybble and the latter in the least significant
    /// two bits of the high nybble.
    ///
    /// The soundness of the property accessor methods depends on this field
    /// only containing valid encodings of each of the two enums.
    raw: u8,
}

impl CharProperties {
    /// Returns a new [CharProperties] value representing a tuple of the
    /// two given property values.
    pub const fn new(gcb: GCBProperty, incb: InCBProperty) -> Self {
        Self {
            raw: gcb as u8 | incb as u8,
        }
    }

    /// Returns the relevant character properties for the given character,
    /// represented as a [`u8char`] value.
    ///
    /// This performs a table lookup using a trie embedded in this library.
    pub const fn for_u8char(c: u8char) -> Self {
        Self {
            raw: table::graphemes_lookup(c),
        }
    }

    /// Returns the relevant character properties for the given character,
    /// represented as a [`char`] value.
    ///
    /// This performs a table lookup using a trie embedded in this library.
    /// The trie is optimized for lookup by [`u8char`], so this function
    /// converts to that representation first as a convenience but it's
    /// better to pass an existing `u8char` value if you happen to have one.
    pub const fn for_char(c: char) -> Self {
        Self {
            raw: table::graphemes_lookup(u8char::from_char(c)),
        }
    }

    /// Returns the [`GCBProperty`] value from this tuple.
    pub const fn gcb_property(self) -> GCBProperty {
        // Safety: The low nybble of our raw repr matches its GCBProperty repr.
        let raw = self.raw & 0xf;
        unsafe { core::mem::transmute(raw) }
    }

    /// Returns the [`InCBProperty`] value from this tuple.
    pub const fn incb_property(self) -> InCBProperty {
        // Safety: The selected bits of our raw repr matches its InCBCategory repr.
        let raw = self.raw & 0x30;
        unsafe { core::mem::transmute(raw) }
    }

    /// Returns `true` if the [`GCBProperty`] describes a control character.
    ///
    /// Specifically, this includes [`GCBProperty::Control`],
    /// [`GCBProperty::LF`], and [`GCBProperty::CR`], for the purposes of
    /// activating rules [GB4](https://www.unicode.org/reports/tr29/#GB4) and
    /// [GB5](https://www.unicode.org/reports/tr29/#GB5).
    pub const fn is_any_control(self) -> bool {
        matches!(
            self.gcb_property(),
            GCBProperty::LF | GCBProperty::CR | GCBProperty::Control,
        )
    }

    /// Const-compatible `Eq`.
    pub const fn eq(self, other: Self) -> bool {
        self.raw == other.raw
    }
}

#[cfg(test)]
#[allow(unused, non_upper_case_globals)]
impl CharProperties {
    pub(crate) const None: Self = Self::gcb_only(GCBProperty::None);
    pub(crate) const CR: Self = Self::gcb_only(GCBProperty::CR);
    pub(crate) const Control: Self = Self::gcb_only(GCBProperty::Control);
    pub(crate) const Extend: Self = Self::gcb_only(GCBProperty::Extend);
    pub(crate) const ExtendedPictographic: Self = Self::gcb_only(GCBProperty::ExtendedPictographic);
    pub(crate) const L: Self = Self::gcb_only(GCBProperty::L);
    pub(crate) const LF: Self = Self::gcb_only(GCBProperty::LF);
    pub(crate) const LV: Self = Self::gcb_only(GCBProperty::LV);
    pub(crate) const LVT: Self = Self::gcb_only(GCBProperty::LVT);
    pub(crate) const Prepend: Self = Self::gcb_only(GCBProperty::Prepend);
    pub(crate) const RegionalIndicator: Self = Self::gcb_only(GCBProperty::RegionalIndicator);
    pub(crate) const SpacingMark: Self = Self::gcb_only(GCBProperty::SpacingMark);
    pub(crate) const T: Self = Self::gcb_only(GCBProperty::T);
    pub(crate) const V: Self = Self::gcb_only(GCBProperty::V);
    pub(crate) const ZWJ: Self = Self::gcb_only(GCBProperty::ZWJ);

    const fn gcb_only(gcb: GCBProperty) -> Self {
        Self::new(gcb, InCBProperty::None)
    }
}

#[cfg(test)]
pub(crate) mod test_table;
