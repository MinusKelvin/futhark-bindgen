pub(crate) use std::collections::BTreeMap;

mod error;
pub(crate) mod generate;
pub mod manifest;

pub use error::Error;
pub use generate::{Config, Generate, OCaml, Rust};
pub use manifest::Manifest;

#[derive(Debug, serde::Deserialize, PartialEq, Clone, Copy)]
pub enum Backend {
    #[serde(rename = "c")]
    C,

    #[serde(rename = "cuda")]
    CUDA,

    #[serde(rename = "opencl")]
    OpenCL,

    #[serde(rename = "multicore")]
    Multicore,

    #[serde(rename = "python")]
    Python,

    #[serde(rename = "pyopencl")]
    PyOpenCL,
}

impl Backend {
    pub fn to_str(&self) -> &'static str {
        match self {
            Backend::C => "c",
            Backend::CUDA => "cuda",
            Backend::OpenCL => "opencl",
            Backend::Multicore => "multicore",
            Backend::Python => "python",
            Backend::PyOpenCL => "pyopencl",
        }
    }

    pub fn required_c_libs(&self) -> &'static [&'static str] {
        match self {
            Backend::CUDA => &["cuda", "cudart", "nvrtc", "m"],
            Backend::OpenCL => &["OpenCL", "m"],
            _ => &[],
        }
    }
}

#[derive(Debug, Clone)]
pub struct Compiler {
    exe: String,
    backend: Backend,
    src: std::path::PathBuf,
    extra_args: Vec<String>,
    output_dir: std::path::PathBuf,
}

#[derive(Debug, Clone)]
pub struct Library {
    pub manifest: Manifest,
    pub c_file: std::path::PathBuf,
    pub h_file: std::path::PathBuf,
    pub src: std::path::PathBuf,
}

impl Library {
    #[cfg(feature = "build")]
    pub fn link(&self) {
        let project = std::env::var("CARGO_PKG_NAME").unwrap();

        let name = format!("futhark_generate_{project}");

        cc::Build::new()
            .flag("-Wno-unused-parameter")
            .file(&self.c_file)
            .compile(&name);
        println!("cargo:rustc-link-lib={name}");

        let libs = self.manifest.backend.required_c_libs();

        for lib in libs {
            println!("cargo:rustc-link-lib={}", lib);
        }
    }
}

#[cfg(feature = "build")]
pub fn build(
    backend: Backend,
    src: impl AsRef<std::path::Path>,
    dest: impl AsRef<std::path::Path>,
) {
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    println!("{:?}", out);
    let lib = Compiler::new(backend, src)
        .with_output_dir(out)
        .compile()
        .expect("Compilation failed")
        .expect("Unable to find manifest file");

    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join(dest);
    let mut config = Config::new(out).expect("Unable to configure codegen");
    let mut gen = config.detect().expect("Invalid output language");
    gen.generate(&lib, &mut config)
        .expect("Code generation failed");
    lib.link();
}

#[cfg(feature = "build")]
pub fn build_in_out_dir(
    backend: Backend,
    src: impl AsRef<std::path::Path>,
    dest: impl AsRef<std::path::Path>,
) {
    let dest = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join(dest);
    build(backend, src, dest)
}

impl Compiler {
    pub fn new(backend: Backend, src: impl AsRef<std::path::Path>) -> Compiler {
        Compiler {
            exe: String::from("futhark"),
            src: src.as_ref().to_path_buf(),
            extra_args: Vec::new(),
            output_dir: src
                .as_ref()
                .canonicalize()
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf(),
            backend,
        }
    }

    pub fn with_executable_name(mut self, name: impl AsRef<str>) -> Self {
        self.exe = name.as_ref().into();
        self
    }

    pub fn with_extra_args(mut self, args: Vec<String>) -> Self {
        self.extra_args = args;
        self
    }

    pub fn with_output_dir(mut self, dir: impl AsRef<std::path::Path>) -> Self {
        self.output_dir = dir.as_ref().to_path_buf();
        self
    }

    pub fn compile(&self) -> Result<Option<Library>, Error> {
        let output = &self
            .output_dir
            .join(self.src.with_extension("").file_name().unwrap());
        let ok = std::process::Command::new(&self.exe)
            .arg(self.backend.to_str())
            .args(&self.extra_args)
            .args(&["-o", &output.to_string_lossy()])
            .arg("--lib")
            .arg(&self.src)
            .status()?
            .success();

        if !ok {
            return Err(Error::CompilationFailed);
        }

        match &self.backend {
            Backend::Python | Backend::PyOpenCL => return Ok(None),
            _ => (),
        }

        let manifest = Manifest::parse_file(output.with_extension("json"))?;
        let c_file = output.with_extension("c");
        let h_file = output.with_extension("h");
        Ok(Some(Library {
            manifest,
            c_file,
            h_file,
            src: self.src.clone(),
        }))
    }
}
