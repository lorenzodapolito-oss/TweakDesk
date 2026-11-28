//! Incorpora l'icona e il manifest (amministratore richiesto sempre) nell'eseguibile Windows
//! (se non riesce, il programma si compila lo stesso).

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=assets/app.manifest");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set_manifest_file("assets/app.manifest");
        if let Err(e) = res.compile() {
            println!("cargo:warning=icona/manifest dell'exe non incorporati: {e}");
        }
    }
}
