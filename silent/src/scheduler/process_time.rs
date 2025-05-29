use chrono::{DateTime, Local, Utc};
use cron::Schedule;
use serde::{Serialize, Serializer};
use std::str::FromStr;

#[derive(Debug, Clone)]
pub enum ProcessTime {
    Datetime(DateTime<Local>),
    Crontab(Box<Schedule>),
}

impl Serialize for ProcessTime {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            ProcessTime::Datetime(d) => serializer.serialize_str(&d.to_string()),
            ProcessTime::Crontab(s) => serializer.serialize_str(&s.to_string()),
        }
    }
}

impl TryFrom<String> for ProcessTime {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match DateTime::<Local>::from_str(&value) {
            Ok(datetime) => Ok(ProcessTime::Datetime(datetime)),
            Err(_) => Ok(ProcessTime::Crontab(Box::from(Schedule::from_str(&value)?))),
        }
    }
}

impl TryFrom<&str> for ProcessTime {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match DateTime::<Local>::from_str(value) {
            Ok(datetime) => Ok(ProcessTime::Datetime(datetime)),
            Err(_) => Ok(ProcessTime::Crontab(Box::from(Schedule::from_str(value)?))),
        }
    }
}

impl TryFrom<DateTime<Local>> for ProcessTime {
    type Error = anyhow::Error;

    fn try_from(value: DateTime<Local>) -> Result<Self, Self::Error> {
        Ok(ProcessTime::Datetime(value))
    }
}

impl TryFrom<DateTime<Utc>> for ProcessTime {
    type Error = anyhow::Error;

    fn try_from(value: DateTime<Utc>) -> Result<Self, Self::Error> {
        Ok(ProcessTime::Datetime(value.into()))
    }
}

impl ProcessTime {
    pub(crate) fn is_active(&self) -> bool {
        match self {
            ProcessTime::Datetime(datetime) => {
                datetime.timestamp_millis() <= Local::now().timestamp_millis()
            }
            ProcessTime::Crontab(crontab) => crontab.includes(Local::now()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_time() {
        let datetime = Local::now();
        let process_time =
            ProcessTime::Datetime(datetime + chrono::TimeDelta::try_seconds(10).unwrap());
        assert!(!process_time.is_active());
        let process_time = ProcessTime::try_from(datetime).unwrap();
        assert!(process_time.is_active());
        let process_time =
            ProcessTime::Crontab(Box::from(Schedule::from_str("* * * * * *").unwrap()));
        assert!(process_time.is_active());
        let process_time =
            ProcessTime::Crontab(Box::from(Schedule::from_str("0 0 0 1 1 ? 2015").unwrap()));
        assert!(!process_time.is_active());
        assert!(ProcessTime::try_from("2023-01-01T00:00:00Z".to_string()).is_ok());
        assert!(ProcessTime::try_from("2023-01-01 00:00:00").is_err());
    }
}
