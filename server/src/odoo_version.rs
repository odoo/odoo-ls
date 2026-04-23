use std::fmt;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OdooVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl OdooVersion {
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self { major, minor, patch }
    }
}

impl fmt::Display for OdooVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl PartialEq<(u32, u32)> for OdooVersion {
    fn eq(&self, other: &(u32, u32)) -> bool {
        self.major == other.0 && self.minor == other.1 && self.patch == 0
    }
}

impl PartialEq<OdooVersion> for (u32, u32) {
    fn eq(&self, other: &OdooVersion) -> bool {
        other == self
    }
}

impl PartialOrd<(u32, u32)> for OdooVersion {
    fn partial_cmp(&self, other: &(u32, u32)) -> Option<std::cmp::Ordering> {
        (self.major, self.minor, self.patch).partial_cmp(&(other.0, other.1, 0))
    }
}

impl PartialOrd<OdooVersion> for (u32, u32) {
    fn partial_cmp(&self, other: &OdooVersion) -> Option<std::cmp::Ordering> {
        (self.0, self.1, 0).partial_cmp(&(other.major, other.minor, other.patch))
    }
}
