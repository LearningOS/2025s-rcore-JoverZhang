use crate::{
    mm::{read_va, write_va, MapPermission, VirtAddr},
    task::{
        change_program_brk, current_mmap, current_munmap, exit_current_and_run_next,
        get_syscall_count, suspend_current_and_run_next,
    },
    timer::get_time_us,
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");

    let us = get_time_us();
    let t = TimeVal {
        sec: us / 1_000_000,
        usec: us % 1_000_000,
    };

    write_va(VirtAddr::from(ts as usize), &t).unwrap();

    0
}

/// TODO: Finish sys_trace to pass testcases
/// HINT: You might reimplement it with virtual memory management.
pub fn sys_trace(trace_request: usize, id: usize, data: usize) -> isize {
    match trace_request {
        // read
        0 => match read_va::<u8>(VirtAddr::from(id as usize)) {
            Some(val) => val as isize,
            None => -1,
        },
        // write
        1 => match write_va::<u8>(VirtAddr::from(id as usize), &(data as u8)) {
            Some(_) => 0,
            None => -1,
        },
        // syscall
        2 => get_syscall_count(id),
        _ => -1,
    }
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    trace!("kernel: sys_mmap NOT IMPLEMENTED YET!");

    let start_va = VirtAddr::from(start as usize);
    if !start_va.aligned() {
        return -1;
    }
    let end_va = VirtAddr::from(start as usize + len);

    if prot > 7 || prot <= 0 {
        return -1;
    }
    let mut map_perm: MapPermission = MapPermission::U;
    // readable
    if (prot & 0b001) != 0 {
        map_perm |= MapPermission::R;
    }
    // writable
    if (prot & 0b010) != 0 {
        map_perm |= MapPermission::W;
    }
    // executable
    if (prot & 0b100) != 0 {
        map_perm |= MapPermission::X;
    }

    match current_mmap(start_va, end_va, map_perm) {
        Ok(_) => 0,
        Err(e) => {
            trace!("kernel: sys_mmap failed: {}", e);
            -1
        },
    }
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!("kernel: sys_munmap");
    let start_va = VirtAddr::from(start as usize);
    if !start_va.aligned() {
        return -1;
    }
    let end_va = VirtAddr::from(start as usize + len);

    match current_munmap(start_va, end_va) {
        Ok(_) => 0,
        Err(e) => {
            trace!("kernel: sys_munmap failed: {}", e);
            -1
        },
    }
}
/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
