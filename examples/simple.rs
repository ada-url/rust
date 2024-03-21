use ada_url::Url;

fn main() {
    let url = Url::parse("http://www.google:8080/love#drug", None).expect("bad url");

    println!("port: {:?}", url.port());
    println!("hash: {:?}", url.hash());
    println!("pathname: {:?}", url.pathname());
    println!("href: {:?}", url.href());

    let mut url = url;
    url.set_port(Some("9999")).expect("bad port");
    println!("href: {:?}", url.href());
}
