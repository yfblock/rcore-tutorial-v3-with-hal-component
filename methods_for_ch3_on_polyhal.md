## 基于polyhal的rcore tutorial ch3

1、迁移ch2的工作；

2、为load_app提供合法地址空间，其中包括程序地址空间和堆栈地址空间：

```rust
       let page_table = PageTable::current();
        let Page_Num = APP_SIZE_LIMIT/PAGE_SIZE;
        for i in 0..Page_Num {
            page_table.map_page(VirtPage::from_addr(base_i + PAGE_SIZE * i), frame_alloc_persist().expect("can't allocate frame"), MappingFlags::URWX, MappingSize::Page4KB);
        }
        page_table.map_page(VirtPage::from_addr(0x1_8000_0000 + i*PAGE_SIZE), frame_alloc_persist().expect("can't allocate frame"), MappingFlags::URWX, MappingSize::Page4KB);
        println!("[kernel] Loading app_{}", i);
        info!("app src: {:#x} size: {:#x}", app_start[i], app_start[i + 1] - app_start[i]);
```

​	首先计算出一个app占用的页的context_switch数量，因为给定的APP_SIZE_LIMIT是PAGE_SIZE的整数倍，所以直接作除法即可。对于第i个APP，为其分配以base_i为起点的Page_Num个页来使用。，同时，为其分配一个页作为堆栈使用。对应的，第i个APP执行的起始地址将是base_i，堆栈地址为0x180000000 + i * PAGE_SIZE。

3、与ch2相比，引入了KContext，这是polyhal中封装的内核状态下的上下文，

```rust
use polyhal::{
    read_current_tp, run_user_task, KContext, KContextArgs, TrapFrame, TrapFrameArgs,
};
```

​    接下来，我们不再需要自己实现TaskContext，而是可以用KContext代替，对TaskControlBlock的定义修改为：

```rust
#[derive(Copy, Clone)]
pub struct TaskControlBlock {
    pub task_status: TaskStatus,
    pub task_cx: KContext,
}
```

此后，switch操作可以直接使用polyhal中封装好的接口：

```rust
unsafe { context_switch(idle_task_cx_ptr, next_task_cx_ptr, token) }
```

其中，context_switch与context_switch_pt的区别在于是否切换页表，这里还不涉及页表的切换，因此使用context_switch。

另外记录一下今天出现的git方面的问题，因为忘记在.gitignore中加入qemu.log和fs-img.img，所以上传文件过大提交失败，又因为它们已经进入commit所以即使删除后再git add . + git commit也无法成功，采用的方式是先在本地另一个目录下暂存修改过的文件，再git reset到提交大文件前的commit，最后将保存的文件复制回来，修改.gitignore后再提交一次。

4.后期make run的时候发现发生缺页，原因是在load_app之前先做了清内存操作，在此之前就需要分配空间(load.rs的第80-85行）：

```rust
        for i in 0..Page_Num {
            page_table.map_page(VirtPage::from_addr(base_i + PAGE_SIZE * i), frame_alloc_persist().expect("can't allocate frame"), MappingFlags::URWX, MappingSize::Page4KB);
        }
        page_table.map_page(VirtPage::from_addr(0x1_8000_0000 + i*PAGE_SIZE), frame_alloc_persist().expect("can't allocate frame"), MappingFlags::URWX, MappingSize::Page4KB);
        (base_i..base_i + APP_SIZE_LIMIT)
            .for_each(|addr| unsafe { (addr as *mut u8).write_volatile(0) });
```

5、重点修改的源代码文件是os/task/mod.rs.对TaskControlBlock的初始化操作变成：

```rust
        let mut tasks = Vec::new();
        for i in 0..MAX_APP_NUM{
            tasks.push(TaskControlBlock {
                task_cx: KContext::blank(),
                task_status: TaskStatus::UnInit,
        });}
        for (i, task) in tasks.iter_mut().enumerate() {
            task.task_status = TaskStatus::Ready;
            task.task_cx[KContextArgs::KPC] = task_entry as usize;
            task.task_cx[KContextArgs::KSP] = get_ksp(i);
            task.task_cx[KContextArgs::KTP] = read_current_tp();          
            }
```

其中，task_entry()为：

```rust
fn task_entry() {
    let app_id = TASK_MANAGER.current_task();
    let mut trap_cx = TrapFrame::new();
    trap_cx[TrapFrameArgs::SEPC] = get_base_i(app_id);
    trap_cx[TrapFrameArgs::SP] = 0x1_8000_0000 + (app_id+1)*PAGE_SIZE;
    let ctx_mut = unsafe { (&mut trap_cx as *mut TrapFrame).as_mut().unwrap() };
    loop {
        run_user_task(ctx_mut);
    }
    panic!("Unreachable in batch::run_current_app!");
}
```

get_ksp(i)为获取第i个函数的用户堆栈的栈顶地址。

PS.本实验尚未加入时钟处理。
