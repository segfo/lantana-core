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
use std::sync::{Arc,Mutex,mpsc};
use std::sync::atomic::{Ordering,AtomicBool};
use std::collections::HashMap;
use std::thread::{Thread,ThreadId,JoinHandle};
mod builder;
use crate::builder::thread_observer::*;
use crate::api::core_services_impl::*;

fn observer_activate(method:&'static str,lib:&common::PluginInfo,observer:Arc<Mutex<HashMap<ThreadId,ThreadObserver>>>)->std::result::Result<(),Box<std::error::Error>>{
    // 特定時間以上動作するライブラリを警告する
    let mut t = observer.lock().unwrap();
    // 監視対象のライブラリのパラメータを設定
    let builder = ThreadObserver::new();
    let observer = builder
        .tid(thread::current().id())
        .dll(lib.get_pluginpath())
        // レスポンスがないと判断し、警告をログに吐くしきい値（時間）
        .noresponse_threshold(30)
        .method(method).build()?;
    //　監視対象をハッシュテーブルに追加する。
    t.insert(observer.get_tid(),observer);
    Ok(())
}

fn observer_deactivate(observer:Arc<Mutex<HashMap<ThreadId,ThreadObserver>>>){
    let mut t = observer.lock().unwrap();
    t.remove(&thread::current().id());
}

/***
 * method: メソッド名
 * args: 引数
 * lib: 実行中のプラグイン情報
 * observer: 監視を実行するオブザーバが使用するハッシュマップ
 ***/
fn pluginrun_with_observer<TArgs,TReturn>(
    method:&'static str,
    args:TArgs,
    lib:&common::PluginInfo,
    observer:Arc<Mutex<HashMap<ThreadId,ThreadObserver>>>)->std::result::Result<TReturn,Box<std::error::Error>>{
    observer_activate(method, lib, observer.clone())?;
    // 走らせる
    let r = lib.get_libloader().exec_module::<TArgs,TReturn>(method,args)?;
    observer_deactivate(observer.clone());
    Ok(r)
}
#[derive(Debug)]
enum MessageDispachId{
    Plugin(DispatcherControlMessage),SysInfo(CoreControllMessage)
}

#[derive(Debug,Clone)]
struct PluginExecInfo{
    behavior:Arc<iris_api::require::Plugin+Sync+Send>,
    tid:Option<ThreadId>,
}

#[derive(Debug)]
enum PluginState{
    InitializeDone,DispatchMessage
}

#[derive(Debug)]
struct DispatcherControlMessage{
    state:PluginState,
    plugin:Option<Arc<common::PluginInfo>>,
    execinfo:PluginExecInfo,
}
#[derive(Debug)]
enum CoreCtrlKind{
    CORE_CLOSING,PLUGIN_PROCESSING_COMPLETE,PLUGIN_ACTIVATE
}
#[derive(Debug)]
struct CoreControllMessage{
    message_kind:CoreCtrlKind,
    dispatch_id:Option<Box<MessageDispachId>>,
}

fn create_dummy_object()->Arc<iris_api::require::Plugin + Sync + Send >{
    #[derive(Debug)]
    struct DummyPlugin;
    impl iris_api::require::Plugin for DummyPlugin{
        fn core_ending_session(&self)->Result<(),Box<std::error::Error>>{
            Ok(())
        }
        fn get_behavior(&self)->iris_api::require::ImplBehavior{
            struct Test{}
            impl iris_api::require::A for Test{
                fn init_a(&self)->u32{0}
            }
            iris_api::require::ImplBehavior::FunctionA(Arc::new(Test{}))
        }
    }
    Arc::new(DummyPlugin{})
}

fn main() {
    let mut init_data = common::application_init();
    let plugin_instances = common::init_plugin_modules(&mut init_data);

    let t_observer = Arc::new(Mutex::new(HashMap::new()));
    let cloneobserver = t_observer.clone();
    thread::spawn(move || {thread_lock_detector(cloneobserver);});

    let mut threadlist = HashMap::new();
    let (tx, rx) = mpsc::channel();
    let tx = Arc::new(Mutex::new(tx));
    let ctrlc_tx=tx.clone();

//    let core_closing = Arc::new(AtomicBool::new(false));
    ctrlc::set_handler(move || {
    //    if core_closing.load(Ordering::Relaxed)==false{
            let _ = ctrlc_tx.lock().unwrap().send(
                MessageDispachId::SysInfo(
                    CoreControllMessage{
                        message_kind:CoreCtrlKind::CORE_CLOSING,
                        dispatch_id:None
                    }
                )
            );
//            core_closing.store(true,Ordering::Relaxed);
//        }
    }).unwrap_or_else(|e|log::warn!("{}",e));
    // 全プラグインの初期化処理
    for lib in plugin_instances.clone(){
        let t_observer = t_observer.clone();
        let data = init_data.clone();
        let tx = tx.clone();
        let lib_clone = lib.clone();
        let thread = thread::spawn(move || {
            let basedir=data.get_install_directory();
            let plugindir=data.get_plugin_directory();
            let p = iris_api::require::CoreData{basedir:basedir,plugindir:plugindir,core_services:LantanaCoreServices::new(lib.get_pluginpath())};
            // 走らせる
            let plugin = match pluginrun_with_observer::<iris_api::require::CoreData,Result<Arc<iris_api::require::Plugin + Sync + Send >,Box<std::error::Error>>>(PLUGIN_ENTRY_POINT,p,&lib,t_observer){
                // メソッドの実行までに成功した？
                Ok(behavior)=>{
                    // プラグインが成功した？
                    match behavior{
                        Ok(behavior)=>behavior,
                        Err(e)=>{
                            log::warn!("{} : Plugin initialize failure.(cause : {})",lib.get_pluginpath().to_str().unwrap(),e);
                            create_dummy_object()
                        }
                    }
                },
                Err(_e)=>{
                    log::warn!("{} : Entry point not implements.",lib.get_pluginpath().to_str().unwrap());
                    create_dummy_object()
                }
            };
            let tx = tx.lock().unwrap();
            let _ = tx.send(
                MessageDispachId::Plugin(
                    DispatcherControlMessage{
                        state:PluginState::InitializeDone,
                        plugin:None,
                        execinfo:PluginExecInfo{
                            tid:Some(thread::current().id()),
                            behavior:plugin.clone()
                        }
                    }
                ));
        });
        threadlist.insert(thread.thread().id(),lib_clone);
    }
    // 何もプラグインがなければ終わり。
    if plugin_instances.len() == 0 {
        return;
    }

    let (dispacher_tx, dispatcher_rx) = mpsc::channel();
    plugin_message_dispacher(tx,dispatcher_rx,t_observer.clone());
    let mut activated = 0;
    // メッセージループ：メッセージを受信して、いい感じに処理をする。
    'finish:loop{
        // Globalの初期化が終わったモジュールから順次、
        // モジュール固有の処理群を走らせる
        for message in rx.recv(){
            match message{
                MessageDispachId::Plugin(ctlmsg)=>{
                    match ctlmsg.state{
                        PluginState::InitializeDone=>{
                            let plugin = threadlist.get(&ctlmsg.execinfo.tid.unwrap()).unwrap();
                            dispacher_tx.send(
                                MessageDispachId::Plugin(
                                    DispatcherControlMessage{
                                        state:ctlmsg.state,
                                        plugin:Some(plugin.clone()),
                                        execinfo:ctlmsg.execinfo
                                    }
                                )
                            );
                        },
                        PluginState::DispatchMessage=>{}
                    }
                },
                // 処理が行われると必ず呼ばれる。条件によりmainを抜ける。
                MessageDispachId::SysInfo(system)=>{
                    match system.message_kind{
                        CoreCtrlKind::CORE_CLOSING=>{
                            println!("sending message(shutting down).");
                            let instances=plugin_instances.clone();
                            // ディスパッチャに対して、シャットダウンメッセージを送る
                            // ディスパッチャからアクティベート済みプラグインに対して更にメッセージが送られる。
                            let _ = dispacher_tx.send(
                                // システム情報
                                MessageDispachId::SysInfo(
                                    CoreControllMessage{
                                        // 要因：システムシャットダウン
                                        message_kind:CoreCtrlKind::CORE_CLOSING,
                                        dispatch_id:None
                                    }
                                )
                            );
                        },
                        CoreCtrlKind::PLUGIN_PROCESSING_COMPLETE=>{
                            activated-=1;
                            if activated==0{
                                break 'finish;
                            }
                        },
                        CoreCtrlKind::PLUGIN_ACTIVATE=>activated+=1,
                    }
                }
            }
        }
    }
    println!("closing core.");
}

fn plugin_activate(
    tx:&std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Sender<MessageDispachId>>>,
    plugins:&mut Mutex<Vec<Arc<DispatcherControlMessage>>>,
    msg:Arc<DispatcherControlMessage>){
    let mut plugins = plugins.lock().unwrap();
    plugins.push(msg);
    let tx = tx.lock().unwrap();
    let _ = tx.send(
        MessageDispachId::SysInfo(
            CoreControllMessage{
                message_kind:CoreCtrlKind::PLUGIN_ACTIVATE,
                dispatch_id:None
            }
        )
    );
}

fn plugin_deactivate(
    tx:&std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Sender<MessageDispachId>>>,
    plugins:&Mutex<Vec<Arc<DispatcherControlMessage>>>,
    observer:Arc<Mutex<HashMap<ThreadId,ThreadObserver>>>){
    let mut plugins = plugins.lock().unwrap();
    for plugin in plugins.iter(){
        let tx = tx.clone();
        let plugin = plugin.clone();

        let observer = observer.clone();
        // 終了時処理
        thread::spawn(move || {
            match &plugin.plugin{
                Some(lib)=>{
                    // 終了メソッドを実行する前に、監視ルーチンに対して登録を行う
                    match observer_activate("core_ending_session", &lib.clone(), observer.clone()){
                        Ok(_)=>{
                            // 実行
                            plugin.execinfo.behavior.core_ending_session();
                            // 監視ルーチンから当該メソッドを削除する。
                            observer_deactivate(observer);
                            // コア本体に対して完了を通知
                            let tx = tx.lock().unwrap();
                            let _ = tx.send(
                                MessageDispachId::SysInfo(
                                    CoreControllMessage{
                                        message_kind:CoreCtrlKind::PLUGIN_PROCESSING_COMPLETE,
                                        dispatch_id:None
                                    }
                                )
                            );
                        },
                        Err(e)=>{log::warn!("prepare_exec_module() function. (cause : {})",e)}
                    }
                },
                None=>{log::error!("library info not found!");}
            };
        });
    }
    plugins.clear();
}

fn plugin_message_dispacher_impl(
    tx:std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Sender<MessageDispachId>>>,
    rx:std::sync::mpsc::Receiver<MessageDispachId>,
    observer:Arc<Mutex<HashMap<ThreadId,ThreadObserver>>>){
    let mut plugins = Mutex::new(Vec::new());
    loop{
        // 受信したプラグインの情報をとりあえず表示しておく
        for msg in rx.recv(){
            // ディスパッチャは、Coreの指令によりプラグインの任意の関数を別スレッドにて実行する。
            match msg{
                MessageDispachId::Plugin(execinfo)=>{
                    match execinfo.state{
                        PluginState::InitializeDone=>{
                            plugin_activate(&tx,&mut plugins,Arc::new(execinfo));
                        },
                        PluginState::DispatchMessage=>{}
                    }
                },
                MessageDispachId::SysInfo(sysinfo)=>{
                    match sysinfo.message_kind{
                        CoreCtrlKind::CORE_CLOSING=>{
                            let observer = observer.clone();
                            plugin_deactivate(&tx,&plugins,observer);
                        },
                        _ =>{},
                    }
                }
            }
        }
    }
}

fn plugin_message_dispacher(tx:std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Sender<MessageDispachId>>>,rx:std::sync::mpsc::Receiver<MessageDispachId>,observer:Arc<Mutex<HashMap<ThreadId,ThreadObserver>>>)->JoinHandle<()>{
    thread::spawn(move || {plugin_message_dispacher_impl(tx,rx,observer)})
}

// LibLoadingの拡張
trait LibLoadingExt{
    fn exec_module<TA,TR>(&self,name:&str,args:TA)->std::result::Result<TR,Box<std::error::Error>>;
}

impl LibLoadingExt for libloading::Library{
    fn exec_module<TArgs,TReturn>(&self,name:&str,args: TArgs)->std::result::Result<TReturn,Box<std::error::Error>>{
        let func:libloading::Symbol<fn(TArgs)->TReturn> = unsafe { match self.get(name.as_bytes()) {
            Ok(func)=>func,
            Err(e)=>{return Err(Box::new(e));}
        }};
        Ok(func(args))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn config(){
    }
}
