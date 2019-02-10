use libloading;
mod common;
mod const_values;
mod load_config;
use std::path::{Path,PathBuf};
use std::fs::DirEntry;
use std::{env,thread, time};
#[macro_use]
use log;
use log4rs;
use crate::const_values::*;
use crate::load_config::*;
use iris_api::*;
use std::sync::{Arc,Mutex};
use std::sync::mpsc::channel;
use std::collections::HashMap;
use std::thread::{Thread,ThreadId,JoinHandle};

// ながーい処理時間のスレッドを検出する。
fn thread_lock_detector(th_list:Arc<Mutex<HashMap<ThreadId,ThreadObserver>>>){
    while true{
        {
            let mut list = th_list.lock().unwrap();
            for val in list.values_mut(){
                val.count+=1;
                if val.count%(LONG_WAIT_THREAD_SECS+1) == 0{
                    log::warn!("{}::{}();  ({:?})thread long wait!",
                        val.dll.clone().unwrap().to_str().unwrap(),
                        val.method,
                        val.threadid.unwrap());
                    val.count=0;
                }
            }
        }
        thread::sleep(time::Duration::from_millis(ONE_SEC));
    }
}

#[derive(Debug)]
enum ThreadState{
    ALIVE,DEAD
}

struct ThreadObserver{
    threadid:Option<ThreadId>,
    dll:Option<PathBuf>,
    count:u32,
    method:&'static str,
    state:ThreadState
}

struct ThreadObserverBuilder{
    to:ThreadObserver
}

#[derive(Debug)]
struct BuilderError{
    kind:BuilderErrorKind
}

impl BuilderError{
    fn new()->Self{
        BuilderError{kind:BuilderErrorKind::None}
    }
}
#[derive(Debug)]
enum BuilderErrorKind{
    None,InvalidParameter
}

impl std::fmt::Display for BuilderError{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self.kind {
            BuilderErrorKind::None=>write!(f, "No error"),
            BuilderErrorKind::InvalidParameter=>write!(f, "It may not satisfy the parameter requirement required for constructing the structure.")
        }
    }
}

impl std::error::Error for BuilderError{
    fn description(&self) -> &str{
        match self.kind{
            BuilderErrorKind::None=>"No error",
            BuilderErrorKind::InvalidParameter=>"Invalid parameter"
        }
    }
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)>{
        None
    }

}

impl ThreadObserverBuilder{
    fn new()->Self{
        ThreadObserverBuilder{
            to:ThreadObserver{
                threadid:None,
                dll:None,
                method:"",
                count:0,
                state:ThreadState::ALIVE
            }
        }
    }
    fn tid(mut self,id:ThreadId)->Self{
        self.to.threadid = Some(id);
        self
    }
    fn dll(mut self,path:PathBuf)->Self{
        self.to.dll = Some(path);
        self
    }
    fn method(mut self,method:&'static str)->Self{
        self.to.method = method;
        self
    }
    fn build(mut self)->Result<ThreadObserver,BuilderError>{
        let mut err = BuilderError::new();
        err.kind=BuilderErrorKind::InvalidParameter;
        if self.to.threadid == None{return Err(err);}
        if self.to.dll == None{return Err(err);}
        if self.to.method == ""{return Err(err);}
        Ok(self.to)
    }
}

impl ThreadObserver{
    fn new()->ThreadObserverBuilder{
        ThreadObserverBuilder::new()
    }
}

/***
 * method: メソッド名
 * args: 引数
 * lib: 実行中のプラグイン情報
 * observer: 監視を実行するオブザーバが使用するハッシュマップ
 ***/
fn pluginrun_with_observer<TArgs,TReturn>(method:&'static str,args:TArgs,lib:&common::PluginInfo,observer:Arc<Mutex<HashMap<ThreadId,ThreadObserver>>>)->TReturn{
    {
        // 特定時間以上動作するライブラリを警告する
        let mut t = observer.lock().unwrap();
        // 監視対象のライブラリのパラメータを設定
        let mut builder = ThreadObserver::new();
        let observer = builder
            .tid(thread::current().id())
            .dll(lib.get_pluginpath())
            .method(method).build().unwrap();
        //　監視対象をハッシュテーブルに追加する。
        t.insert(observer.threadid.unwrap(),observer);
    }
    // 走らせる
    let r = lib.get_instance().exec_module::<TArgs,TReturn>(method,args);
    let mut t = observer.lock().unwrap();
    t.remove(&thread::current().id());
    r
}

fn lanatana_entry() {
    let mut init_data = common::application_init();
    let plugin_instances = common::init_plugin_modules(&mut init_data);

    let map=HashMap::new();
    let t_observer = Arc::new(Mutex::new(map));

    let cloneobserver = t_observer.clone();
    thread::spawn(move || {thread_lock_detector(cloneobserver);});

    let mut threadlist = Vec::new();
    for lib in plugin_instances{
        let t_observer = t_observer.clone();
        let data = init_data.clone();
        let thread = thread::spawn(move || {
            let basedir=data.get_install_directory();
            let plugindir=data.get_plugin_directory();
            let p = iris_api::base::ParentData{basedir:basedir,plugindir:plugindir};
            // 走らせる
            pluginrun_with_observer::<iris_api::base::ParentData,u32>(PLUGIN_ENTRY_POINT,p,&lib,t_observer);
        });
        threadlist.push(thread);
    }
    
    for th in threadlist{
        th.join();
    }
}

trait LibLoadingExt{
    fn exec_module<TA,TR>(&self,name:&str,args:TA)->TR;
}

impl LibLoadingExt for libloading::Library{
    fn exec_module<TArgs,TReturn>(&self,name:&str,args: TArgs)->TReturn{
        let func: libloading::Symbol<fn(TArgs)->TReturn> = unsafe { self.get(name.as_bytes()).unwrap() };
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
