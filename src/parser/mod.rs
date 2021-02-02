use derive_more::{Display, From};
use rayon::prelude::*;
use serde_yaml;

use super::{
    client::Client,
    device::Device,
    file::{DeviceParserEntry, OSParserEntry, RegexFile, UserAgentParserEntry},
    os::OS,
    parser::{
        device::Error as DeviceError, os::Error as OSError,
        user_agent::Error as UserAgentError,
    },
    user_agent::UserAgent,
    Parser, SubParser,
};

mod device;
mod os;
mod user_agent;

#[derive(Debug, Display, From)]
pub enum Error {
    IO(std::io::Error),
    Yaml(serde_yaml::Error),
    Device(DeviceError),
    OS(OSError),
    UserAgent(UserAgentError),
}

/// Handles the actual parsing of a user agent string by delegating to
/// the respective `SubParser`
#[derive(Debug)]
pub struct UserAgentParser {
    device_matchers: Vec<device::Matcher>,
    os_matchers: Vec<os::Matcher>,
    user_agent_matchers: Vec<user_agent::Matcher>,
}

impl Parser for UserAgentParser {
    /// Returns the full `Client` info when given a user agent string
    fn parse(&self, user_agent: &str) -> Client {
        let device = self.parse_device(&user_agent);
        let os = self.parse_os(&user_agent);
        let user_agent = self.parse_user_agent(&user_agent);

        Client {
            device,
            os,
            user_agent,
        }
    }

    /// Returns just the `Device` info when given a user agent string
    fn parse_device(&self, user_agent: &str) -> Device {
        self.device_matchers
            .iter()
            .filter_map(|matcher| matcher.try_parse(&user_agent))
            .take(1)
            .next()
            .unwrap_or_default()
    }

    /// Returns just the `OS` info when given a user agent string
    fn parse_os(&self, user_agent: &str) -> OS {
        self.os_matchers
            .iter()
            .filter_map(|matcher| matcher.try_parse(&user_agent))
            .take(1)
            .next()
            .unwrap_or_default()
    }

    /// Returns just the `UserAgent` info when given a user agent string
    fn parse_user_agent(&self, user_agent: &str) -> UserAgent {
        self.user_agent_matchers
            .iter()
            .filter_map(|matcher| matcher.try_parse(&user_agent))
            .take(1)
            .next()
            .unwrap_or_default()
    }
}

impl UserAgentParser {
    /// Attempts to construct a `UserAgentParser` from the path to a file
    pub fn from_yaml(path: &str) -> Result<UserAgentParser, Error> {
        let file = std::fs::File::open(path)?;
        Ok(UserAgentParser::from_file(file)?)
    }

    /// Attempts to construct a `UserAgentParser` from a slice of raw bytes. The
    /// intention with providing this function is to allow using the
    /// `include_bytes!` macro to compile the `regexes.yaml` file into the
    /// the library by a consuming application.
    ///
    /// ```rust
    /// # use uaparser::*;
    /// let regexes = include_bytes!("../../src/core/regexes.yaml");
    /// let parser = UserAgentParser::from_bytes(regexes);
    /// ```
    pub fn from_bytes(bytes: &[u8]) -> Result<UserAgentParser, Error> {
        let regex_file: RegexFile = serde_yaml::from_slice(bytes)?;
        Ok(UserAgentParser::try_from(regex_file)?)
    }

    /// Attempts to construct a `UserAgentParser` from a reference to an open
    /// `File`. This `File` should be a the `regexes.yaml` depended on by
    /// all the various implementations of the UA Parser library.
    pub fn from_file(file: std::fs::File) -> Result<UserAgentParser, Error> {
        let regex_file: RegexFile = serde_yaml::from_reader(file)?;
        Ok(UserAgentParser::try_from(regex_file)?)
    }

    pub fn try_from(regex_file: RegexFile) -> Result<UserAgentParser, Error> {
        let device_matchers = regex_file
            .device_parsers
            .into_par_iter()
            .try_fold(
                || Vec::new(),
                |mut v, parser| -> Result<_, Error> {
                    let matcher = device::Matcher::try_from(parser)?;
                    v.push(matcher);
                    Ok(v)
                },
            )
            .try_reduce(
                || Vec::new(),
                |mut v1, v2| {
                    v1.extend(v2);
                    Ok(v1)
                },
            )?;

        let os_matchers = regex_file
            .os_parsers
            .into_par_iter()
            .try_fold(
                || Vec::new(),
                |mut v, parser| -> Result<_, Error> {
                    let matcher = os::Matcher::try_from(parser)?;
                    v.push(matcher);
                    Ok(v)
                },
            )
            .try_reduce(
                || Vec::new(),
                |mut v1, v2| {
                    v1.extend(v2);
                    Ok(v1)
                },
            )?;

        let user_agent_matchers = regex_file
            .user_agent_parsers
            .into_par_iter()
            .try_fold(
                || Vec::new(),
                |mut v, parser| -> Result<_, Error> {
                    let matcher = user_agent::Matcher::try_from(parser)?;
                    v.push(matcher);
                    Ok(v)
                },
            )
            .try_reduce(
                || Vec::new(),
                |mut v1, v2| {
                    v1.extend(v2);
                    Ok(v1)
                },
            )?;

        // for parser in regex_file.device_parsers.into_iter() {
        //     device_matchers.push(device::Matcher::try_from(parser)?);
        // }

        // for parser in regex_file.os_parsers.into_iter() {
        //     os_matchers.push(os::Matcher::try_from(parser)?);
        // }

        // for parser in regex_file.user_agent_parsers.into_iter() {
        //     user_agent_matchers.push(user_agent::Matcher::try_from(parser)?);
        // }

        Ok(UserAgentParser {
            device_matchers,
            os_matchers,
            user_agent_matchers,
        })
    }
}

pub(self) fn none_if_empty<T: AsRef<str>>(s: T) -> Option<T> {
    if !s.as_ref().is_empty() {
        Some(s)
    } else {
        None
    }
}

pub(self) fn replace(replacement: &str, captures: &fancy_regex::Captures) -> String {
    if replacement.contains('$') && captures.len() > 0 {
        (1..=captures.len())
            .fold(replacement.to_owned(), |state: String, i: usize| {
                let group = captures.get(i).map(|x| x.as_str()).unwrap_or("");
                state.replace(&format!("${}", i), &group)
            })
            .trim()
            .to_owned()
    } else {
        replacement.to_owned()
    }
}
