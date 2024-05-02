//! Loading user applications into memory

/// Get the total number of applications.
use alloc::vec::Vec;
use lazy_static::*;
///get app number
pub fn get_num_app() -> usize {
    extern "C" {
        fn _num_app();
    }
    unsafe { (_num_app as usize as *const usize).read_volatile() }
}
/// get applications data
pub fn get_app_data(app_id: usize) -> &'static [u8] {
    extern "C" {
        fn _num_app();
    }
    let num_app_ptr = _num_app as usize as *const usize;
    let num_app = get_num_app();
    let app_start = unsafe { core::slice::from_raw_parts(num_app_ptr.add(1), num_app + 1) };
    assert!(app_id < num_app);
    unsafe {
        core::slice::from_raw_parts(
            app_start[app_id] as *const u8,
            app_start[app_id + 1] - app_start[app_id],
        )
    }
}

// 获取所有app的名字
lazy_static! {
    ///All of app's name
    static ref APP_NAMES: Vec<&'static str> = {
        let num_app = get_num_app();
        extern "C" {
            fn _app_names();
        }
        let mut start = _app_names as usize as *const u8;
        let mut v = Vec::new();
        unsafe {
            for _ in 0..num_app {
                // 两个指针（start 和 end）计算出当前字符串的长度
                let mut end = start;
                // 找到字符串的结束位置
                while end.read_volatile() != b'\0' {
                    end = end.add(1);
                }
                // 创建一个切片
                let slice = core::slice::from_raw_parts(start, end as usize - start as usize);
                // 将切片转换为字符串
                let str = core::str::from_utf8(slice).unwrap();
                v.push(str);
                start = end.add(1);
            }
        }
        v
    };
}

#[allow(unused)]
///get app data from name
/// 按照应用的名字来查找获得应用的 ELF 数据
pub fn get_app_data_by_name(name: &str) -> Option<&'static [u8]> {
    let num_app = get_num_app();
    (0..num_app)
        .find(|&i| APP_NAMES[i] == name)   // 找到名字相同的应用
        .map(get_app_data)
}
///list all apps
/// 内核初始化时被调用，它可以打印出所有可用的应用的名字
pub fn list_apps() {
    println!("/**** APPS ****");
    for app in APP_NAMES.iter() {
        println!("{}", app);
    }
    println!("**************/");
}
