use ada_url::Url;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut u = Url::parse("http://www.google:8080/love#drug", None).expect("bad url");
    println!("port: {:?}", u.port());
    println!("hash: {:?}", u.hash());
    println!("pathname: {:?}", u.pathname());
    println!("href: {:?}", u.href());
    u.set_port(Some("9999"))?;
    println!("href: {:?}", u.href());

    Ok(())
}
