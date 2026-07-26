import sys

with open("kernel/src/shell.rs", "r") as f:
    text = f.read()

text = text.replace("sdk::sys_get_user(&mut user_buf, &mut user_len);", "crate::syscall::sys_get_user(user_buf.as_mut_ptr() as u64, &mut user_len as *mut _ as u64);")
text = text.replace("sdk::sys_set_user(canonical);", "crate::syscall::sys_set_user(canonical.as_ptr() as u64, canonical.len() as u64);")
text = text.replace("sdk::sys_net_setdns(&ip);", "crate::syscall::sys_net_setdns(ip.as_ptr() as u64);")
text = text.replace("sdk::SysIfconfigInfo::zeroed()", "crate::syscall::SysIfconfigInfo { has_nic: 0, mac: [0; 6], ip: [0; 4], gateway: [0; 4], dns: [0; 4] }")
text = text.replace("sdk::sys_net_ifconfig(&mut info)", "crate::syscall::sys_net_ifconfig(&mut info as *mut _ as u64)")
text = text.replace("sdk::sys_net_ping(&ip, &mut reply_buf);", "crate::syscall::sys_net_ping(ip.as_ptr() as u64, reply_buf.as_mut_ptr() as u64);")
text = text.replace("sdk::sys_net_httpget(&ip, port, path, &mut out_buf);", "crate::syscall::sys_net_httpget(ip.as_ptr() as u64, port as u64, path.as_ptr() as u64, path.len() as u64, out_buf.as_mut_ptr() as u64, out_buf.len() as u64);")

with open("kernel/src/shell.rs", "w") as f:
    f.write(text)
