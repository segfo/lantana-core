#![allow(dead_code)]
use std::path::PathBuf;

pub struct LantanaCoreServices{
    dll_name:PathBuf
}
impl LantanaCoreServices{
    pub fn new(p:PathBuf)->Box<Self>{
        Box::new(LantanaCoreServices{dll_name:p})
    }
}

impl iris_api::core::CoreServices for LantanaCoreServices{
    fn write_console(&self,s:String){
        println!("{:?} : {}",self.dll_name,s);
    }
    fn read_console(&self)->Result<String,std::io::Error>{
        let mut s=String::new();
        std::io::stdin().read_line(&mut s)?;
        Ok(s)
    }
}
