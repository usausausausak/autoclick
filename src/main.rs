extern crate xcb;
extern crate failure;

use std::{env, time, thread};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use failure::{Error, err_msg};

const CLICK_DELAY: u64 = 300;
const STOP_THRESHOLD: i16 = 30;

fn pointer_click(conn: &xcb::Connection) {
    xcb::test::fake_input(conn, xcb::BUTTON_PRESS, 1,
                          0, xcb::WINDOW_NONE, 0, 0, 0);
    conn.flush();
    xcb::test::fake_input(conn, xcb::BUTTON_RELEASE, 1,
                          0, xcb::WINDOW_NONE, 0, 0, 0);
    conn.flush();
}

fn pointer_position(conn: &xcb::Connection, root: xcb::Window)
    -> Result<(i16, i16), xcb::GenericError>
{

    let cookie = xcb::query_pointer(&conn, root);
    cookie.get_reply().map(|r| (r.root_x(), r.root_y()))
}

fn auto_click(running: &Arc<AtomicBool>) -> Result<u64, Error> {
    let (conn, _) = xcb::Connection::connect(None)?;

    let root = conn.get_setup().roots()
                               .next().ok_or(err_msg("query root error"))?
                               .root();

    let pointer_start = pointer_position(&conn, root)?;

    #[cfg(debug_assertions)]
    println!("pointer at {}, {}", pointer_start.0, pointer_start.1);

    let is_pointer_moved = |x: i16, y: i16| -> bool {
        let dx = x - pointer_start.0;
        let dy = y - pointer_start.1;
        dx.abs() >= STOP_THRESHOLD || dy.abs() >= STOP_THRESHOLD
    };

    let mut clicked_time: u64 = 0;
    // Keep generate click event
    // until the main thread unset the running flag,
    // or the user moved the pointer.
    while running.load(Ordering::SeqCst) {
        clicked_time += 1;
        pointer_click(&conn);

        // Avoid getting the x server too busy.
        thread::sleep(time::Duration::from_millis(CLICK_DELAY));

        let (x, y) = pointer_position(&conn, root)?;
        if is_pointer_moved(x, y) {
            #[cfg(debug_assertions)]
            println!("moved to {}, {}", x, y);
            break;
        }
    }

    Ok(clicked_time)
}

fn main() {
    let run_time = env::args().skip(1).next()
                                      .and_then(|s| s.parse::<u64>().ok());

    // This flag indicate the running state of application,
    // may unset in both the main thread and the click thread.
    let running = Arc::new(AtomicBool::new(true));

    let click_running = running.clone();
    let click_thread = thread::spawn(move || {
        // TODO: Maybe print result in the main thread is better?
        match auto_click(&click_running) {
            Ok(click_time) => println!("clicked {} times", click_time),
            Err(err) => eprintln!("{}", err),
        }

        // Notify the main thread don't sleep anymore.
        click_running.store(false, Ordering::SeqCst);
    });

    // The main thread will get to sleep until timeout,
    // or when the click thread has ended.
    // Sleep time is split in short periods to get response quickly.
    match run_time {
        Some(mut run_time) if run_time > 0 => {
            #[cfg(debug_assertions)]
            println!("click {} seconds", run_time);

            while run_time > 0 && running.load(Ordering::SeqCst) {
                thread::sleep(time::Duration::from_secs(1));
                run_time -= 1;
            }
        },
        _ => {
            #[cfg(debug_assertions)]
            println!("click until pointer moved");

            while running.load(Ordering::SeqCst) {
                thread::sleep(time::Duration::from_millis(CLICK_DELAY));
            }
        },
    }

    // Stop the click thread.
    running.store(false, Ordering::SeqCst);

    let _ = click_thread.join();
}
