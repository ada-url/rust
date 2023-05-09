#![feature(test)]

extern crate test;

// Taken from https://github.com/Brooooooklyn/ada-url/blob/main/ada/bench/parse.rs

use ada_url::Url;
use test::{black_box, Bencher};

const URL: &[&str] = &[
    "https://www.google.com/",
    "webhp?hl=en&amp;ictx=2&amp;sa=X&amp;ved=0ahUKEwil_",
    "oSxzJj8AhVtEFkFHTHnCGQQPQgI",
    "https://support.google.com/websearch/",
    "?p=ws_results_help&amp;hl=en-CA&amp;fg=1",
    "https://en.wikipedia.org/wiki/Dog#Roles_with_humans",
    "https://www.tiktok.com/@aguyandagolden/video/7133277734310038830",
    "https://business.twitter.com/en/help/troubleshooting/",
    "how-twitter-ads-work.html?ref=web-twc-ao-gbl-adsinfo&utm_source=twc&utm_",
    "medium=web&utm_campaign=ao&utm_content=adsinfo",
    "https://images-na.ssl-images-amazon.com/images/I/",
    "41Gc3C8UysL.css?AUIClients/AmazonGatewayAuiAssets",
    "https://www.reddit.com/?after=t3_zvz1ze",
    "https://www.reddit.com/login/?dest=https%3A%2F%2Fwww.reddit.com%2F",
    "postgresql://other:9818274x1!!@localhost:5432/",
    "otherdb?connect_timeout=10&application_name=myapp",
    "http://192.168.1.1",            // ipv4
    "http://[2606:4700:4700::1111]", // ipv6
];

#[bench]
fn ada_parse(b: &mut Bencher) {
    b.iter(|| {
        URL.iter().for_each(|url| {
            let _ = black_box(Url::parse(url, None));
        });
    });
}

#[bench]
fn servo_parse(b: &mut Bencher) {
    b.iter(|| {
        URL.iter().for_each(|url| {
            let _ = black_box(url::Url::parse(url));
        });
    });
}
