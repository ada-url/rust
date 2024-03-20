use ada_url::Url;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = Url::parse("http://www.google:8080/love#drug", None).expect("bad url");

    println!("port: {:?}", url.port());
    println!("hash: {:?}", url.hash());
    println!("pathname: {:?}", url.pathname());
    println!("href: {:?}", url.href());

    let mut url = url;

    #[cfg(feature = "std")]
    url.set_port(Some("9999"))?;

    #[cfg(not(feature = "std"))]
    url.set_port(Some("9999")).unwrap();

    println!("href: {:?}", url.href());

    Ok(())
}
