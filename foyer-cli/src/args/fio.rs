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

use std::{collections::HashSet, fmt::Display, process::Command, str::FromStr};

use anyhow::anyhow;

use crate::args::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IoEngine {
    IoUring,
    Psync,
    LibAio,
    PosixAio,
}

impl FromStr for IoEngine {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "io_uring" => Ok(Self::IoUring),
            "psync" => Ok(Self::Psync),
            "libaio" => Ok(Self::LibAio),
            "posixaio" => Ok(Self::PosixAio),
            other => Err(other.to_string()),
        }
    }
}

impl Display for IoEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IoEngine::IoUring => write!(f, "io_uring"),
            IoEngine::Psync => write!(f, "psync"),
            IoEngine::LibAio => write!(f, "libaio"),
            IoEngine::PosixAio => write!(f, "posixaio"),
        }
    }
}

impl IoEngine {
    pub fn asynchronous(&self) -> bool {
        match self {
            IoEngine::IoUring => true,
            IoEngine::Psync => true,
            IoEngine::LibAio => true,
            IoEngine::PosixAio => false,
        }
    }
}

#[derive(Debug)]
pub struct Fio {
    io_engines: HashSet<IoEngine>,
}

impl Fio {
    pub fn init() -> Result<Self> {
        if !Self::available() {
            return Err(Error::FioNotAvailable);
        }

        let io_engines = Self::list_io_engines()?;

        Ok(Self { io_engines })
    }

    fn available() -> bool {
        let output = match Command::new("fio").arg("--version").output() {
            Ok(output) => output,
            Err(_) => return false,
        };
        output.status.success()
    }

    pub fn io_engines(&self) -> &HashSet<IoEngine> {
        &self.io_engines
    }

    fn list_io_engines() -> Result<HashSet<IoEngine>> {
        let output = Command::new("fio").arg("--enghelp").output()?;
        if !output.status.success() {
            return Err(anyhow!("fail to get available io engines with fio").into());
        }

        let io_engines = String::from_utf8_lossy(&output.stdout)
            .split('\n')
            .skip(1)
            .map(|s| s.trim())
            .filter(|s: &&str| !s.is_empty())
            .flat_map(|s| s.parse())
            .collect();

        Ok(io_engines)
    }
}
