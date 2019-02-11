#![allow(unused_imports)]
#![allow(dead_code)]
use libloading;
mod common;
mod const_values;
mod load_config;
mod api;
use std::fs::DirEntry;
use std::thread;
use log;
use log4rs;
use crate::const_values::*;
use iris_api::*;
use std::sync::{Arc,Mutex};
use std::collections::HashMap;
use std::thread::{Thread,ThreadId,JoinHandle};
mod builder;
use crate::builder::thread_observer::*;
use crate::api::core_services_impl::*;


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
        let builder = ThreadObserver::new();
        let observer = builder
            .tid(thread::current().id())
            .dll(lib.get_pluginpath())
            // レスポンスがないと判断し、警告をログに吐くしきい値（時間）
            .noresponse_threshold(30)
            .method(method).build().unwrap();
        //　監視対象をハッシュテーブルに追加する。
        t.insert(observer.get_tid(),observer);
    }
    // 走らせる
    let r = lib.get_instance().exec_module::<TArgs,TReturn>(method,args);
    let mut t = observer.lock().unwrap();
    t.remove(&thread::current().id());
    r
}

// エントリーポイント
fn lanatana_entry() {
    let mut init_data = common::application_init();
    let plugin_instances = common::init_plugin_modules(&mut init_data);

    let map = HashMap::new();
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
            let p = iris_api::core::ParentData{basedir:basedir,plugindir:plugindir,core_api:LantanaCoreServices::new(lib.get_pluginpath())};
            // 走らせる
            let _ = pluginrun_with_observer::<iris_api::core::ParentData,Result<iris_api::core::Initialize,Box<std::error::Error>>>(PLUGIN_ENTRY_POINT,p,&lib,t_observer);
        });
        threadlist.push(thread);
    }
   
    for th in threadlist{
        th.join().unwrap();
    }
}

// LibLoadingの拡張
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
