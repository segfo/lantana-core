use libloading;
mod common;
mod const_values;
mod load_config;
use std::path::{Path,PathBuf};
use std::fs::DirEntry;
use std::env;
#[macro_use]
use log;
use log4rs;
use crate::const_values::*;
use crate::load_config::*;

fn lanatana_entry() {
    
    let mut init_data = common::application_init();
    let plugin_instances = common::init_plugin_modules(&mut init_data);
    for lib in plugin_instances{
        lib.exec_module::<libloading::Library,u32>("hello",&lib);
    }
}

trait LibLoadingExt{
    fn exec_module<TA,TR>(&self,name:&str,args:&TA)->TR;
}

impl LibLoadingExt for libloading::Library{
    fn exec_module<TA,TR>(&self,name:&str,args: &TA)->TR{
        let func: libloading::Symbol<fn(&TA)->TR> = unsafe { self.get(name.as_bytes()).unwrap() };
        func(args)
    }
}

#[cfg(test)]
mod tests {
    use crate::lanatana_entry;
    #[test]
    fn exec_main() {
        lanatana_entry();
    }

    #[test]
    fn config(){
    }
}
