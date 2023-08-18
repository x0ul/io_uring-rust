use io_uring::{opcode, types, IoUring};
use std::os::unix::io::AsRawFd;
use std::error::Error;
use libc;
use errno;

fn main() -> Result<(), Box<dyn Error>> {
    println!("Starting!");
    let mut ring = IoUring::new(8)?;

    let socket_e = opcode::Socket::new(libc::PF_INET, libc::SOCK_STREAM, 0).build().user_data(1);
    unsafe {
        ring.submission()
            .push(&socket_e)
            .expect("submission queue is full");
    }

    ring.submit_and_wait(1)?;

    let sock_cqe = ring.completion().next().unwrap();
    assert_eq!(sock_cqe.user_data(), 1, "not the cqe for socket()");
    let sock_fd = sock_cqe.result();
    assert!(sock_fd != -1, "sock fd {sock_fd} < 0, errno is {}", errno::errno());
    let sock_fd = io_uring::types::Fd(sock_fd as _);

    let sockaddr_in: *const libc::sockaddr_in = &libc::sockaddr_in {
        sin_family: libc::AF_INET.try_into()?,
        sin_port: 80_u16.to_be(),
        sin_addr: libc::in_addr { s_addr: u32::to_be(93 << 24 | 184 << 16 | 216 << 8 | 34) },
        sin_zero: [0; 8]
    };
    let sockaddr = unsafe { std::mem::transmute::<*const libc::sockaddr_in, *const libc::sockaddr>(sockaddr_in) };
    let addrlen = 16;
    let connect_e = opcode::Connect::new(types::Fd(sock_fd.0.as_raw_fd()), sockaddr, addrlen).build().user_data(2);
    unsafe {
        ring.submission()
            .push(&connect_e)
            .expect("submission queue is full");
    }
    println!("connecting...");
    ring.submit_and_wait(1)?;

    let connect_cqe = ring.completion().next().unwrap();
    assert_eq!(connect_cqe.user_data(), 2, "not the cqe for connect()");
    assert_eq!(connect_cqe.result(), 0, "connect failed: errno {}", errno::errno());
    println!("connected!");

    let http_get = "GET / HTTP/1.1\nHost: example.com\nUser-Agent: cody-rhea/42069\nAccept: */*\n\n".as_bytes();
    let mut buf = vec![0; 1024 * 32]; // This is big enough for
 // example.com, but to do larger sites we'd want to break up the
 // `recv` into multiple calls
    let send_e = opcode::Send::new(sock_fd, http_get.as_ptr(), http_get.len() as _)
        .build()
        .flags(io_uring::squeue::Flags::IO_LINK)
        .user_data(3);
    let recv_e = opcode::Recv::new(sock_fd, buf.as_mut_ptr(), buf.len() as _)
        .build()
        .flags(io_uring::squeue::Flags::IO_LINK)
        .user_data(4);
    unsafe {
        ring.submission()
            .push(&send_e)
            .expect("submission queue is full");
        ring.submission()
            .push(&recv_e)
            .expect("submission queue is full");
    }
    ring.submit_and_wait(2)?;

    println!("sent and waiting!");

    for cqe in ring.completion() {
        match cqe.user_data() {
            3 => {
                println!("sent the data! result was {}", cqe.result());
            },
            4 => {
                println!("received a response! The result was {}", cqe.result());
                let chars = std::str::from_utf8(&buf[0..cqe.result().try_into()?]).unwrap();
                println!("the response was contained: {chars}");
            },
            _ => println!("wtf"),
        }
    }

    println!("We did it!");
    Ok(())
}
