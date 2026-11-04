//! Build-host release assessment only. This tool ships no controller service or
//! paid-operation authority and is not included in the runtime distribution.
mod archive;
mod candidate;
mod coverage;
mod files;
mod gate;
mod installed;
mod lock;
mod manifest;
mod process;
mod sbom;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

fn run() -> files::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let p = |i: usize| std::path::Path::new(&args[i]);
    match (args.first().map(String::as_str),args.len()) {
        (Some("seal"),3)=>println!("{}",candidate::seal(p(1),p(2))?),
        (Some("verify"),3)=>{manifest::verify(p(1),&args[2])?;println!("distribution integrity verified; no execution authority granted");},
        (Some("archive"),5)=>println!("{}",archive::create(p(1),p(2),&args[3],p(4))?),
        (Some("verify-archive"),4)=>{archive::verify(p(1),&args[2],p(3))?;println!("archive metadata and contents verified without extraction");},
        (Some("coverage"),2)=>println!("{}",serde_json::to_string_pretty(&coverage::generate(p(1))?).map_err(|_| "coverage serialization failed")?),
        (Some("installed-prefix"),3)=>println!("{}",serde_json::to_string_pretty(&installed::run(p(1),&args[2])?).map_err(|_| "installed report serialization failed")?),
        (Some("require-offline"),3)=>{
            manifest::verify(p(1),&args[2])?;
            // No public report import, Boolean override, or synthetic grant. Current
            // installed-system gaps are mandatory and cannot be waived by metadata.
            return Err("offline release acceptance blocked: installed-system journey and mandatory scope evidence incomplete; see COVERAGE.json and EVIDENCE.json".into());
        },
        _=>return Err("usage: rolvya-release seal SOURCE DIST | verify DIST MANIFEST_SHA256 | archive SOURCE DIST MANIFEST_SHA256 NEW_ARCHIVE | verify-archive DIST MANIFEST_SHA256 ARCHIVE | coverage SOURCE | require-offline DIST MANIFEST_SHA256".into()),
    }
    Ok(())
}
