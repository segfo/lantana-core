#![allow(dead_code)]
use std::path::PathBuf;
use crate::builder::*;
use crate::const_values::*;
use std::thread::*;
use std::sync::{Arc,Mutex};
use std::collections::HashMap;

#[derive(Debug,Clone)]
pub enum ThreadState{
    ALIVE,DEAD
}
#[derive(Debug)]
pub struct ThreadObserver{
    threadid:Option<ThreadId>,
    dll:Option<PathBuf>,
    count_threshold:u32,
    count:u32,
    method:&'static str,
    state:ThreadState
}

impl ThreadObserver{
    pub fn new()->ThreadObserverBuilder{
        ThreadObserverBuilder::new()
    }
    pub fn get_tid(&self)->ThreadId{
        self.threadid.unwrap()
    }
    pub fn get_dll(&self)->PathBuf{
        self.dll.clone().unwrap()
    }
    pub fn get_method(&self)->&'static str{
        self.method
    }
    pub fn get_state(&self)->ThreadState{
        self.state.clone()
    }
}

pub struct ThreadObserverBuilder{
    to:ThreadObserver
}

impl ThreadObserverBuilder{
    pub fn new()->Self{
        ThreadObserverBuilder{
            to:ThreadObserver{
                threadid:None,
                dll:None,
                method:"",
                count_threshold:LONG_WAIT_THREAD_SECS,
                count:0,
                state:ThreadState::ALIVE
            }
        }
    }
    pub fn tid(mut self,id:ThreadId)->Self{
        self.to.threadid = Some(id);
        self
    }
    pub fn dll(mut self,path:PathBuf)->Self{
        self.to.dll = Some(path);
        self
    }
    pub fn method(mut self,method:&'static str)->Self{
        self.to.method = method;
        self
    }
    // レスポンスがないと判断するしきい値（単位は時間・秒）
    pub fn noresponse_threshold(mut self,sec:u32)->Self{
        self.to.count_threshold=sec;
        self
    }
    pub fn build(self)->std::result::Result<ThreadObserver,BuilderError>{
        let mut err = BuilderError::new();
        err.kind=BuilderErrorKind::InvalidParameter;
        if self.to.threadid == None{return Err(err);}
        if self.to.dll == None{return Err(err);}
        if self.to.method == ""{return Err(err);}
        Ok(self.to)
    }
}

// ながーい処理時間のスレッドを検出する。
pub fn thread_lock_detector(th_list:Arc<Mutex<HashMap<ThreadId,ThreadObserver>>>){
    loop{
        {
            let mut list = th_list.lock().unwrap();
            for val in list.values_mut(){
                val.count+=1;
                if val.count%val.count_threshold == 0{
                    log::warn!("{}::{}();  ({:?})thread long wait!",
                        val.dll.clone().unwrap().to_str().unwrap(),
                        val.method,
                        val.threadid.unwrap());
                    val.count=0;
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(ONE_SEC));
    }
}
