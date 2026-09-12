//! Validated names and syntactic scopes for shared workspace resources. Handles are callable
//! index names. This crate does not grant access, persist documents, interpret CRDTs, or
//! resolve authenticated identities.

use std::{fmt, str::FromStr};

use thiserror::Error;

const MAX_ADDRESS_LEN: usize = 1024;
const MAX_SEGMENT_LEN: usize = 128;
const MAX_HANDLE_LEN: usize = 64;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AddressError {
    #[error("invalid workspace resource address")]
    InvalidAddress,
    #[error("invalid workspace resource handle")]
    InvalidHandle,
    #[error("invalid workspace resource scope")]
    InvalidScope,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResourceAddress(String);

impl ResourceAddress {
    pub fn parse(value: &str) -> Result<Self, AddressError> {
        if value.len() > MAX_ADDRESS_LEN {
            return Err(AddressError::InvalidAddress);
        }
        let mut parts = value.split('/');
        let valid = parts.next() == Some("ws")
            && parts.next().is_some_and(valid_segment)
            && parts.next().is_some_and(valid_segment)
            && parts.all(valid_segment);
        valid
            .then(|| Self(value.to_owned()))
            .ok_or(AddressError::InvalidAddress)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn workspace_id(&self) -> &str {
        self.0.split('/').nth(1).expect("validated address")
    }
}

impl FromStr for ResourceAddress {
    type Err = AddressError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl fmt::Display for ResourceAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResourceHandle(String);

impl ResourceHandle {
    pub fn parse(value: &str) -> Result<Self, AddressError> {
        if value.len() > MAX_HANDLE_LEN {
            return Err(AddressError::InvalidHandle);
        }
        let mut chars = value.chars();
        let valid = chars.next().is_some_and(|c| c.is_ascii_lowercase())
            && chars
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_'));
        valid
            .then(|| Self(value.to_owned()))
            .ok_or(AddressError::InvalidHandle)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ResourceScope {
    Exact(ResourceAddress),
    Subtree(ResourceAddress),
}

impl ResourceScope {
    pub fn parse(value: &str) -> Result<Self, AddressError> {
        if let Some(base) = value.strip_suffix("/*") {
            return ResourceAddress::parse(base)
                .map(Self::Subtree)
                .map_err(|_| AddressError::InvalidScope);
        }
        ResourceAddress::parse(value)
            .map(Self::Exact)
            .map_err(|_| AddressError::InvalidScope)
    }

    pub fn covers(&self, target: &ResourceAddress) -> bool {
        match self {
            Self::Exact(address) => address == target,
            Self::Subtree(address) => target
                .as_str()
                .strip_prefix(address.as_str())
                .is_some_and(|rest| rest.starts_with('/')),
        }
    }
}

impl FromStr for ResourceScope {
    type Err = AddressError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceBinding {
    handle: ResourceHandle,
    target: ResourceAddress,
}

impl ResourceBinding {
    pub fn new(handle: ResourceHandle, target: ResourceAddress) -> Self {
        Self { handle, target }
    }

    pub fn handle(&self) -> &ResourceHandle {
        &self.handle
    }

    pub fn target(&self) -> &ResourceAddress {
        &self.target
    }
}

fn valid_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SEGMENT_LEN
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b':'))
}

#[cfg(test)]
mod tests;
