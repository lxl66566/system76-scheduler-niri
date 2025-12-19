use std::io::Error;

use log::{error, info};
use niri_ipc::{Event, Response, socket::Socket};
use zbus::blocking::Connection;

// 定义 System76 Scheduler 的 D-Bus 代理接口
// 这样程序就可以通过 D-Bus 与系统调度服务通信
#[zbus::proxy(
    default_service = "com.system76.Scheduler",
    interface = "com.system76.Scheduler",
    default_path = "/com/system76/Scheduler"
)]
trait System76Scheduler {
    /// 告诉调度器哪个 PID 是当前的前台进程，以便优化其性能
    fn set_foreground_process(&self, pid: u32) -> zbus::Result<()>;
}

fn main() -> std::io::Result<()> {
    // 初始化彩色日志输出
    colog::init();

    // 连接到 Niri 窗口管理器的 IPC 套接字
    let mut socket = Socket::connect()?;

    // 连接到 D-Bus 系统总线
    let conn = Connection::system().map_err(Error::other)?;

    // 创建调度器服务的同步（阻塞）客户端代理
    let proxy = System76SchedulerProxyBlocking::new(&conn).map_err(Error::other)?;

    // 向 Niri 发送请求，订阅事件流（EventStream）
    let reply = socket.send(niri_ipc::Request::EventStream)?;

    // 检查 Niri 是否成功处理了事件流请求
    if !matches!(reply, Ok(Response::Handled)) {
        error!("Niri didn't handle event stream request: {reply:?}");
    }

    // 用于在内存中缓存当前所有窗口的信息
    let mut windows = Vec::new();

    // 获取读取事件的闭包
    let mut read_event = socket.read_events();

    // 循环监听从 Niri 传来的事件
    while let Ok(event) = read_event() {
        match event {
            // 当窗口列表发生变化（如打开、关闭窗口）时，更新本地缓存
            Event::WindowsChanged { windows: _windows } => {
                windows = _windows;
            }

            // 当窗口焦点发生变化时（用户切换了窗口）
            Event::WindowFocusChanged { id: Some(id) } => {
                // 在缓存中根据窗口 ID 查找对应的窗口详细信息
                let window = windows.iter().find(|window| window.id == id);

                if let Some(window) = window {
                    // 如果窗口关联了 PID
                    if let Some(pid) = window.pid {
                        // 调用 D-Bus 接口，通知 System76 Scheduler 提升该 PID 的优先级
                        if let Err(why) = proxy.set_foreground_process(pid as u32) {
                            error!("Failed to set foreground process PID: {why}");
                        };
                        info!(
                            "Set window {:?} with PID {} as the foreground process",
                            window.title, pid
                        );
                    }
                }
            }

            // 忽略其他不相关的事件
            _ => (),
        }
    }

    Ok(())
}
