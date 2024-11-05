# 多道批处理系统  
  
## 多个程序是怎么被加载的？  
  
  批处理系统是将多个应用打包加载放置到内存的一段区域，这里具体来说就是一段连续的物理地址，
  根据 app/task 的序号连续的放置。
```rust
# /os/loader.rs
/// Load nth user app at
/// [APP_BASE_ADDRESS + n * APP_SIZE_LIMIT, APP_BASE_ADDRESS + (n+1) * APP_SIZE_LIMIT).
pub fn load_apps() {
    extern "C" {
        fn _num_app();
    }
    let num_app_ptr = _num_app as usize as *const usize;
    let num_app = get_num_app();
    let app_start = unsafe { core::slice::from_raw_parts(num_app_ptr.add(1), num_app + 1) };
    // clear i-cache first
    unsafe {
        asm!("fence.i");
    }
    // load apps
    for i in 0..num_app {
        let base_i = get_base_i(i);
        // clear region
        (base_i..base_i + APP_SIZE_LIMIT)
            .for_each(|addr| unsafe { (addr as *mut u8).write_volatile(0) });
        // load app from data section to memory
        let src = unsafe {
            core::slice::from_raw_parts(app_start[i] as *const u8, app_start[i + 1] - app_start[i])
        };
        let dst = unsafe { core::slice::from_raw_parts_mut(base_i as *mut u8, src.len()) };
        dst.copy_from_slice(src);
    }
}
```
   `core::slice::from_raw_parts` 用于从裸指针地址位置开始，第二个参数长度信息，返回一个 `slice` 的可变或不可变引用。
   `get_base_i` 计算方式即将 `APP_BASE_ADDRESS`(0x80400000) 应用起始位置（也就是内存的位置了）加上 `i * APP_SIZE_LIMIT(4096)`。为了加载进来，
   首先将 base_i 开始的位置清零，这里用到了闭包直接写入0，再从外部 app_start 至下一个应用的内容转成切片。
   app 在没有被加载之前的大小可能是连续放置的，不一定是4k的大小，所以用的是 `app_start[i + 1] - app_start[i]` 。
   最后 `copy_from_slice` 复制到内存位置.  
     
## 有了多任务批处理，怎么实现任务之间的切换？  
  
  首先第一个疑问是：  
  
### 在哪里发生的任务切换？  
  
  这里说的任务可以认为就是前文说的放置在内存批处理的应用，他们各自被 usize 大小的序号标记，
  序号呢也和 `TaskManagerInner` 中 tasks 数组的下标相对应。这一点也可以从 task 的 trait 返回类型上得到验证。
  
```rust
pub struct TaskManagerInner {
    /// task list
    tasks: [TaskControlBlock; MAX_APP_NUM],
    /// id of current `Running` task
    current_task: usize,
}
```  

  任务切换的流程是： U 模式下进入 S 模式，发生 trap 切换，在 S 态下有特别的函数 _switch 函数。
  **_switch内将当前任务的上下文和下一个要执行的任务的上下文进行分别的保存(sd)和加载(ld)**，**此过程发生sp ra寄存器的切换，也就是两条任务执行流被切换了，后面执行的时候就是按照next任务sp ra执行**，
  *特别的，ra被切换了那么switch执行完了就会进入B的执行流。*这个过程可能会套娃，
  但是每一个上文都被保存在TaskControlBlock中，最终会返回到当前任务。
  > 为什么有 _switch 呢？我们可以看到系统调用sys_call中例如yield都会调用TASKMANANGER的find_next方法，
  这里面就会有任务的切换  
    
### 怎么对对任务块和管理器的结构体理解？  
  
```rust
/// Inner of Task Manager
pub struct TaskManagerInner {
    /// task list
    tasks: [TaskControlBlock; MAX_APP_NUM],
    /// id of current `Running` task
    current_task: usize,
}

pub struct TaskControlBlock {
    /// The task status in it's lifecycle
    pub task_status: TaskStatus,
    /// The task context
    pub task_cx: TaskContext,
    /// The fist time task start
    pub task_start_time: usize,
    /// The syscall times
    pub syscall_times: [u32; MAX_SYSCALL_NUM]
}
```
  TaskManager是总调控器，巨大的以任务块为单位的任务数组，同时记录着当前执行任务块。TaskControlBlock记录每个具体任务的状态，
  以及最关键的切换时的上下文就保存在 TaskContext 中，里面包含ra sp 和12个通用寄存器（这相比 TrapContext 小多了，没有CSR和临时寄存器组）
  
### 以及一些其他问题的思考  
  
  1. TrapContext和TaskContext都要保存通用寄存器？最后 _restore 又将寄存器读出来，是不是没有必要保存在TaskContext呢？  
  A：TrapContext中保存的寄存器记录了应用陷入S特权级之前的CPU状态，而TaskContext则可以看成一个应用在S特权级进行Trap处理的过程中调用__switch之前的CPU状态。
  当恢复TaskContext之后会继续进行Trap处理，而__restore恢复TrapContext之后则是会回到用户态执行应用。  
  *另外，保存TrapContext之后进行Trap处理的时候，s0-s11寄存器可能会被覆盖，后面进行任务切换时这些寄存器会被保存到TaskContext中，也就是说这两个Context中的s0-s11也很可能是不同的。*  
   
  2. 举个例子：从A到B的任务切换  
  A: A yield
    Trap: __alltraps切换权限级到内核, sepc保存应用程序A下一条指令  
    Switch:   
        ra保存A内核switch的返回地址  
        在内核栈内调换任务, 从B的ra接着执行  
        连接到__restore, 有两种方式  
            初始化设置TaskContext.ra = __restore  
            执行__alltraps里的trap_handler后面接着的代码:__restore  
    Trap: __restore 从内核栈恢复应用程序环境, 从B的sepc接着执行
  
  3. 展示流程图吧  
  ![task切换](./figure/lab1/image.png)