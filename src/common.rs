use std::io;
use std::fs::{self};
use std::path::{Path,PathBuf};
use std::ffi::OsStr;
use std::collections::HashMap;
use crate::const_values::*;
#[cfg(windows)]
use kernel32;
use winapi;
use log;
use log4rs;

trait OsStrExtension{
    fn to_wide_chars(&self) -> Vec<u16>;
}

#[cfg(windows)]
fn from_wide_ptr(ptr: *const u16,size:isize) -> String {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    unsafe {
        assert!(!ptr.is_null());
        let len = (0..size).position(|i| *ptr.offset(i) == 0).unwrap();
        let slice = std::slice::from_raw_parts(ptr, len);
        OsString::from_wide(slice).to_string_lossy().into_owned()
    }
}

#[cfg(windows)]
impl OsStrExtension for &str {
    fn to_wide_chars(&self) -> Vec<u16> {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        OsStr::new(self).encode_wide().chain(Some(0).into_iter()).collect::<Vec<_>>()
    }
}

#[derive(Clone,Debug)]
pub struct PluginState{}

impl PluginState{
    fn new()->Self{
        PluginState{}
    }
}

#[derive(Debug,Clone)]
pub struct InitData{
    module_path:PathBuf,
    loaded_modules:HashMap<String,PluginState>,
    config:Option<Box<GlobalConfig>>,
}

impl InitData{
    pub fn get_module_path(&self)->PathBuf{
        self.module_path.clone()
    }
    pub fn get_install_directory(&self)->PathBuf{
        let mut install_path = self.module_path.clone();
        install_path.pop();
        install_path
    }
    pub fn find_loadmodule(&self,path:&PathBuf)->Option<&PluginState>{
        self.loaded_modules.get(&path.clone().into_os_string().into_string().unwrap())
    }
    pub fn get_plugin_directory(&self)->PathBuf{
        let mut basedir = self.get_install_directory();
        basedir.push(PLUGIN_DIR);
        basedir
    }
}

// DLLサーチパスのカレントディレクトリ無効化
// モジュールの完全修飾パス名を取得
// DLL Hijacking対策
#[cfg(windows)]
fn windows_init()->InitData{
    let mut buf;
    unsafe{
        // 関数：SetDllDirectoryW
        // 成功：非零
        // 失敗：0
        // 詳細：https://docs.microsoft.com/en-us/windows/desktop/api/winbase/nf-winbase-setdlldirectoryw
        let ret = kernel32::SetDllDirectoryW("".to_wide_chars().as_ptr());
        if ret == 0{panic!("SetDllDirectoryW(\"\") failure!");}
        // 関数：SetSearchPathMode
        // 成功：非零
        // 失敗：0
        // 詳細：https://docs.microsoft.com/ja-jp/windows/desktop/api/winbase/nf-winbase-setsearchpathmode
        let ret = kernel32::SetSearchPathMode(0x00000001);
        if ret == 0{panic!("SetSearchPathMode(BASE_SEARCH_PATH_ENABLE_SAFE_SEARCHMODE) failure!");}
        // 関数：GetModuleFileNameW
        // 成功：非零
        // 失敗：0
        //　詳細：https://docs.microsoft.com/en-us/windows/desktop/api/libloaderapi/nf-libloaderapi-getmodulefilenamew
        buf = vec![0u16;4096];
        kernel32::GetModuleFileNameW(std::ptr::null_mut() ,buf.as_mut_ptr(),buf.len() as u32);
        if ret == 0{panic!("GetModuleFileNameW failure!");}
    }
    let execmodule_path_string = from_wide_ptr(buf.as_ptr(),buf.len() as isize).to_owned();
    // 全体設定を読み込む
    InitData{
        module_path: Path::new(&execmodule_path_string).to_path_buf(),
        loaded_modules: HashMap::new(),
        config : None
    }
}

#[cfg(unix)]
fn unix_init()->InitData{
    // 全体設定を読み込む
    InitData{
        module_path: std::fs::read_link("/proc/self/exe").unwrap(),
        loaded_modules: HashMap::new(),
        config : None
    }
}
use crate::load_config::*;
pub fn application_init()->InitData{
    #[cfg(windows)]
    let mut data = windows_init();
    #[cfg(unix)]
    let mut data = unix_init();

    let mut pulgin_dir = data.get_install_directory();
    #[cfg(debug_assertions)]
    {
        debug_path_adjust(&mut pulgin_dir);
    }
    let mut config_dir = pulgin_dir.clone();
    config_dir.push(CONFIG_DIR);
    config_dir.push(LOG_CONF);
    log4rs::init_file(config_dir, Default::default()).unwrap();
    log::info!("booting up");

    let mut file = open(&get_configpath(GLOBAL_CONF)).unwrap();

    let config = match GlobalConfig::load(&mut file){
        Ok(config)=>config,
        Err(_e)=>{
            // なにか読めないエラーが起こった。
            // 1. 古いファイルを日付をつけてバックアップする。
            GlobalConfig::backup(&mut file,CauseKind::BrokenFile);
            // 2. 新しい既定のコンフィグを書き込む
            GlobalConfig::new().store(&mut file).unwrap();
            GlobalConfig::load(&mut file).unwrap()
        }
    };
    data.config = Some(config);
    log::info!("config loaded");
    log::info!("initialize phase done.");
    data
}

trait PathExtention{
    fn is_extension(&self,ext:&str)->bool;
}

impl PathExtention for PathBuf{
    fn is_extension(&self,ext: &str)->bool{
        let ext_self = self.extension().unwrap_or(OsStr::new(""));
        if ext == ext_self{
            true
        }else{
            false
        }
    }
}

pub fn dll_scan(dir: &PathBuf) -> io::Result<Vec<PathBuf>> {
    // ビルド先のOSによってDLLの拡張子を変える。
    // 動的ライブラリの拡張子は将来に渡って変更がないと思われるのでハードコード
    #[cfg(windows)]
    let dll_ext="dll";
    #[cfg(unix)]
    let dll_ext="so";

    let mut dirs = Vec::new();
    let mut files = Vec::new();
    dirs.push(dir.clone());
    while let Some(dir) = dirs.pop(){
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                let is_dll = path.is_extension(dll_ext);
                if path.metadata().unwrap().file_type().is_file()&&is_dll{
                    files.push(path);
                }
            }
        }
    }
    Ok(files)
}

/////////////プラグインロード（初回のみ）//////////////////////
#[cfg(debug_assertions)]
fn debug_path_adjust(path:&mut PathBuf){
    path.pop();
    path.pop();
    path.pop();
}

pub struct PluginInfo{
    lib:libloading::Library,
    dll:PathBuf
}

impl PluginInfo{
    pub fn get_instance(&self)->&libloading::Library{
        &self.lib
    }
    pub fn get_pluginpath(&self)->PathBuf{
        self.dll.clone()
    }
}

pub fn init_plugin_modules(init_data:&mut InitData)->Vec<PluginInfo>{
    let mut pulgin_dir = init_data.get_install_directory();
    
    #[cfg(debug_assertions)]
    {
        debug_path_adjust(&mut pulgin_dir);
    }
    pulgin_dir.push(PLUGIN_DIR);
    let dlllist = match dll_scan(&pulgin_dir){
        Ok(list)=>list,
        Err(_e)=>{
            fs::create_dir(&pulgin_dir).unwrap();
            dll_scan(&pulgin_dir).unwrap()
        }
    };
    let mut plugin_instances = Vec::new();
    for dll in dlllist{
        let lib = match libloading::Library::new(dll.clone()){
            Ok(lib)=>{
                let path=dll.to_str().unwrap().to_owned();
                log::info!("{} がロードされました。",path);
                init_data.loaded_modules.insert(path,PluginState::new());
                lib
            },
            Err(e)=>{
                // GetLastErrorで取得される「%1 は有効なアプリケーションではありません」コード193　が返却されることを想定。
                let path=dll.to_str().unwrap().to_owned();
                log::warn!("{}",e.to_string().replace("%1",&path));
                continue;
            }
        };
        plugin_instances.push(PluginInfo{lib:lib,dll:dll});
    }
    plugin_instances
}