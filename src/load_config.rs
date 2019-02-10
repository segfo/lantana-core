use crate::const_values::*;
use std::io::{Write,Read,Seek,SeekFrom};
use std::error::Error;
use std::path::{Path,PathBuf};
use toml;
use serde_derive::{Serialize, Deserialize};
use std::fs::File;
use std::fs::OpenOptions;
use fs2::FileExt;
use chrono::prelude::*;

pub fn get_configpath(file:&str)->PathBuf{
    let mut path = PathBuf::from(CONFIG_DIR);
    path.push(file);
    path
}

pub fn open(path:&PathBuf)->Result<File,Box<Error>>{
    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(true);
    let file = match options.open(path){
        Ok(file)=>Ok(file),
        Err(e)=>{
            if e.kind()==std::io::ErrorKind::AlreadyExists{
                match OpenOptions::new()
                    .read(true).write(true)
                    .open(path){
                        Ok(file)=>Ok(file),
                        Err(e)=>Err(Box::new(e))
                }
            }else{
                Err(Box::new(e))
            }
        }
    };
    match file{
        Ok(file)=>{
            file.lock_exclusive()?;
            Ok(file)
        },
        Err(e)=>Err(Box::new(e))
    }
}

#[derive(Debug)]
pub enum CauseKind{
    BrokenFile
}

pub trait Accessor{
    fn load(file:&mut File)->Result<Box<Self>,Box<Error>>;
    fn store(self,file:&mut File)->Result<(),Box<Error>>;
}

#[derive(Debug,Serialize,Deserialize,Clone)]
pub struct GlobalConfig{
}

use log4rs;
use log;
impl GlobalConfig{
    pub fn new()->Self{
        GlobalConfig{
            
        }
    }
    pub fn backup(file:&mut File,cause:CauseKind){
        log::warn!("{}",CONF_BACKUPPED);
        log::warn!("{}: {:?}",CAUSE,cause);
        let local: DateTime<Local> = Local::now();
        let date_str = local.format(&format!("{}{}",BROKEN_CONF_BACKUP_PREFIX,GLOBAL_CONF)).to_string();
        let mut backup = OpenOptions::new()
            .read(true).write(true)
            .create_new(true).open(date_str).unwrap();
        let mut buf = String::new();
        file.seek(SeekFrom::Start(0));
        file.read_to_string(&mut buf).unwrap();
        backup.write(buf.as_bytes());
    }
}

impl Accessor for GlobalConfig{
    fn load(file:&mut File)->Result<Box<Self>,Box<Error>>{
        file.seek(SeekFrom::Start(0))?;
        let mut config_string = String::new();
        file.read_to_string(&mut config_string)?;
        let config=toml::from_str(&config_string)?;
        Ok(Box::new(config))
    }
    // ストアしたらselfの所有権を奪う。
    // 新たにインスタンスが必要であれば、再ロードしなければならない。
    fn store(self,file:&mut File)->Result<(),Box<Error>>{
        file.seek(SeekFrom::Start(0))?;
        file.set_len(0)?;
        file.write(toml::to_string(&self)?.as_bytes())?;
        file.flush()?;
        Ok(())
    }
}

