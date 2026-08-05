//! Contract tests for the offline operator release bundle.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tar::{Archive, Builder};
use tempfile::TempDir;

/// Release version exercised by the public pilot artifact tests.
const VERSION: &str = "0.2.1";
/// Fixed valid image digest used only for generated test fixtures.
const IMAGE_DIGEST: &str =
    "sha256:1111111111111111111111111111111111111111111111111111111111111111";

/// One intentionally invalid bundle mutation.
#[derive(Clone, Copy)]
enum Mutation {
    /// Removes an operator file required by the allowlist.
    MissingFile,
    /// Leaves an unresolved release placeholder in documentation.
    Placeholder,
    /// Replaces the immutable image digest with a mutable version tag.
    MutableImage,
    /// Replaces the immutable environment image with a mutable version tag.
    MutableEnvironmentImage,
    /// Replaces the immutable launcher image with a mutable version tag.
    MutableLauncherImage,
    /// Changes an operator documentation link to a missing local file.
    BrokenDocumentationLink,
    /// Changes metadata to a version different from the archive name.
    WrongVersion,
    /// Changes a covered file without updating the internal checksum manifest.
    BadChecksum,
    /// Removes the image digest from release metadata.
    MissingDigest,
}

/// Provides stable labels and checksum behavior for invalid bundle fixtures.
impl Mutation {
    /// Returns a stable directory label for generated mutation evidence.
    fn label(self) -> &'static str {
        match self {
            Self::MissingFile => "missing-file",
            Self::Placeholder => "placeholder",
            Self::MutableImage => "mutable-image",
            Self::MutableEnvironmentImage => "mutable-environment-image",
            Self::MutableLauncherImage => "mutable-launcher-image",
            Self::BrokenDocumentationLink => "broken-documentation-link",
            Self::WrongVersion => "wrong-version",
            Self::BadChecksum => "bad-checksum",
            Self::MissingDigest => "missing-digest",
        }
    }

    /// Returns whether this mutation must intentionally retain stale checksums.
    fn preserves_bad_checksum(self) -> bool {
        matches!(self, Self::BadChecksum)
    }
}

/// Resolves the repository root from this integration test crate.
fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

/// Writes minimal valid SPDX and provenance inputs for the bundle builder.
fn write_release_inputs(root: &Path) -> (PathBuf, PathBuf) {
    let sbom = root.join("sbom.spdx.json");
    let provenance = root.join("provenance.json");
    fs::write(&sbom, r#"{"spdxVersion":"SPDX-2.3"}"#).expect("write SBOM fixture");
    fs::write(&provenance, r#"{"mediaType":"local-test"}"#).expect("write provenance fixture");
    (sbom, provenance)
}

/// Returns the full lowercase source commit used by the generated fixture.
fn source_commit(repository: &Path) -> String {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repository)
        .output()
        .expect("run git rev-parse");
    assert!(output.status.success(), "resolve source commit");
    String::from_utf8(output.stdout)
        .expect("UTF-8 commit")
        .trim()
        .to_owned()
}

/// Builds and copies one valid archive plus its adjacent outer checksum.
fn build_valid_archive(temp: &TempDir) -> PathBuf {
    let repository = repository_root();
    let (sbom, provenance) = write_release_inputs(temp.path());
    let status = Command::new("bash")
        .arg(repository.join("scripts/build-release-bundle.sh"))
        .arg(VERSION)
        .arg(source_commit(&repository))
        .arg("docker.io/ghostframe/local-it-desk")
        .arg(IMAGE_DIGEST)
        .arg(sbom)
        .arg(provenance)
        .current_dir(&repository)
        .status()
        .expect("run release bundle builder");
    assert!(status.success(), "valid release bundle must build");

    let source_archive = repository
        .join("dist")
        .join(format!("local-it-desk-{VERSION}.tar.gz"));
    let archive = temp.path().join(format!("local-it-desk-{VERSION}.tar.gz"));
    fs::copy(&source_archive, &archive).expect("copy valid release archive");
    fs::copy(
        source_archive.with_extension("gz.sha256"),
        archive.with_extension("gz.sha256"),
    )
    .expect("copy valid outer checksum");
    archive
}

/// Runs the independent bundle verifier and returns its captured process output.
fn verify_archive(archive: &Path) -> std::process::Output {
    Command::new("bash")
        .arg(repository_root().join("scripts/verify-release-bundle.sh"))
        .arg(archive)
        .current_dir(repository_root())
        .output()
        .expect("run release bundle verifier")
}

/// Extracts a release archive into one isolated mutation directory.
fn extract_archive(archive: &Path, destination: &Path) -> PathBuf {
    fs::create_dir_all(destination).expect("create mutation directory");
    let compressed = File::open(archive).expect("open valid archive");
    let mut bundle = Archive::new(GzDecoder::new(compressed));
    bundle.unpack(destination).expect("extract valid archive");
    destination.join(format!("local-it-desk-{VERSION}"))
}

/// Collects every regular payload file recursively for checksum regeneration.
fn collect_regular_files(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).expect("read bundle directory") {
        let path = entry.expect("bundle directory entry").path();
        let metadata = fs::symlink_metadata(&path).expect("bundle path metadata");
        if metadata.is_dir() {
            collect_regular_files(&path, files);
        } else if metadata.is_file() && path.file_name().is_none_or(|name| name != "SHA256SUMS") {
            files.push(path);
        }
    }
}

/// Computes one lowercase SHA-256 digest for a regular fixture file.
fn digest_file(path: &Path) -> String {
    let mut file = File::open(path).expect("open checksum input");
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let bytes = file.read(&mut buffer).expect("read checksum input");
        if bytes == 0 {
            break;
        }
        hasher.update(&buffer[..bytes]);
    }
    format!("{:x}", hasher.finalize())
}

/// Rewrites the bundle's internal checksum manifest after a focused mutation.
fn rewrite_internal_checksums(bundle_root: &Path) {
    let mut files = Vec::new();
    collect_regular_files(bundle_root, &mut files);
    files.sort();
    let mut manifest = String::new();
    for path in files {
        let relative = path
            .strip_prefix(bundle_root)
            .expect("relative bundle path");
        manifest.push_str(&format!(
            "{}  ./{}\n",
            digest_file(&path),
            relative.display()
        ));
    }
    fs::write(bundle_root.join("SHA256SUMS"), manifest).expect("rewrite internal checksums");
}

/// Repackages one extracted bundle and writes its matching outer checksum.
fn repack_archive(bundle_root: &Path, archive: &Path) {
    let output = File::create(archive).expect("create mutated archive");
    let encoder = GzEncoder::new(output, Compression::default());
    let mut builder = Builder::new(encoder);
    let root_name = bundle_root.file_name().expect("bundle root name");
    builder
        .append_dir_all(root_name, bundle_root)
        .expect("repack mutated bundle");
    let encoder = builder.into_inner().expect("finish tar archive");
    encoder.finish().expect("finish gzip archive");
    let checksum = format!(
        "{}  {}\n",
        digest_file(archive),
        archive.file_name().expect("archive name").to_string_lossy()
    );
    fs::write(archive.with_extension("gz.sha256"), checksum).expect("write outer checksum");
}

/// Applies one invalid mutation and returns a separately packaged archive.
fn mutated_archive(valid_archive: &Path, temp: &TempDir, mutation: Mutation) -> PathBuf {
    let mutation_root = temp.path().join(mutation.label());
    let bundle_root = extract_archive(valid_archive, &mutation_root);
    match mutation {
        Mutation::MissingFile => {
            fs::remove_file(bundle_root.join("docs/TLS.md")).expect("remove required file");
        }
        Mutation::Placeholder => {
            fs::write(bundle_root.join("release/README.txt"), "CHANGE_ME\n")
                .expect("write placeholder");
        }
        Mutation::MutableImage => {
            let path = bundle_root.join("compose.yaml");
            let compose = fs::read_to_string(&path).expect("read Compose fixture");
            let mutable = compose.replace(
                &format!("docker.io/ghostframe/local-it-desk@{IMAGE_DIGEST}"),
                "docker.io/ghostframe/local-it-desk:0.2.1",
            );
            assert_ne!(compose, mutable, "immutable image fixture must be replaced");
            fs::write(path, mutable).expect("write mutable Compose fixture");
        }
        Mutation::MutableEnvironmentImage => {
            let path = bundle_root.join(".env.example");
            let environment = fs::read_to_string(&path).expect("read environment fixture");
            let mutable = environment.replace(
                &format!("docker.io/ghostframe/local-it-desk@{IMAGE_DIGEST}"),
                "docker.io/ghostframe/local-it-desk:0.2.1",
            );
            assert_ne!(
                environment, mutable,
                "immutable environment image fixture must be replaced"
            );
            fs::write(path, mutable).expect("write mutable environment fixture");
        }
        Mutation::MutableLauncherImage => {
            let path = bundle_root.join("scripts/desk");
            let launcher = fs::read_to_string(&path).expect("read launcher fixture");
            let mutable = launcher.replace(
                &format!("docker.io/ghostframe/local-it-desk@{IMAGE_DIGEST}"),
                "docker.io/ghostframe/local-it-desk:0.2.1",
            );
            assert_ne!(
                launcher, mutable,
                "immutable launcher image fixture must be replaced"
            );
            fs::write(path, mutable).expect("write mutable launcher fixture");
        }
        Mutation::BrokenDocumentationLink => {
            let path = bundle_root.join("docs/RUNBOOK.md");
            let documentation = fs::read_to_string(&path).expect("read runbook fixture");
            let broken = documentation.replace("ROSTER-IMPORT.md", "MISSING-ROSTER-IMPORT.md");
            assert_ne!(
                documentation, broken,
                "linked roster documentation fixture must be replaced"
            );
            fs::write(path, broken).expect("write broken documentation fixture");
        }
        Mutation::WrongVersion => {
            let path = bundle_root.join("release/release-metadata.json");
            let mut metadata: Value =
                serde_json::from_slice(&fs::read(&path).expect("read metadata"))
                    .expect("parse metadata");
            metadata["version"] = Value::String("9.9.9".to_owned());
            fs::write(
                path,
                serde_json::to_vec_pretty(&metadata).expect("serialize metadata"),
            )
            .expect("write wrong-version metadata");
        }
        Mutation::BadChecksum => {
            let path = bundle_root.join("release/README.txt");
            let mut file = fs::OpenOptions::new()
                .append(true)
                .open(path)
                .expect("open checksum-covered file");
            file.write_all(b"checksum mismatch\n")
                .expect("mutate checksum-covered file");
        }
        Mutation::MissingDigest => {
            let path = bundle_root.join("release/release-metadata.json");
            let mut metadata: Value =
                serde_json::from_slice(&fs::read(&path).expect("read metadata"))
                    .expect("parse metadata");
            metadata["image"]
                .as_object_mut()
                .expect("image metadata object")
                .remove("digest");
            fs::write(
                path,
                serde_json::to_vec_pretty(&metadata).expect("serialize metadata"),
            )
            .expect("write digest-free metadata");
        }
    }
    if !mutation.preserves_bad_checksum() {
        rewrite_internal_checksums(&bundle_root);
    }
    let archive = mutation_root.join(format!("local-it-desk-{VERSION}.tar.gz"));
    repack_archive(&bundle_root, &archive);
    archive
}

/// Accepts a valid bundle and rejects each publication contract violation.
#[test]
fn release_bundle_verifier_rejects_invalid_artifacts() {
    let temp = tempfile::tempdir().expect("release bundle test directory");
    let valid_archive = build_valid_archive(&temp);
    let valid_output = verify_archive(&valid_archive);
    assert!(
        valid_output.status.success(),
        "valid bundle rejected: {}",
        String::from_utf8_lossy(&valid_output.stderr)
    );

    for mutation in [
        Mutation::MissingFile,
        Mutation::Placeholder,
        Mutation::MutableImage,
        Mutation::MutableEnvironmentImage,
        Mutation::MutableLauncherImage,
        Mutation::BrokenDocumentationLink,
        Mutation::WrongVersion,
        Mutation::BadChecksum,
        Mutation::MissingDigest,
    ] {
        let archive = mutated_archive(&valid_archive, &temp, mutation);
        let output = verify_archive(&archive);
        assert!(
            !output.status.success(),
            "{} mutation unexpectedly passed",
            mutation.label()
        );
    }
}
