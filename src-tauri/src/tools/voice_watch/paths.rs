pub fn paths_equal(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.replace('\\', "/");
    #[cfg(windows)]
    {
        norm(a).eq_ignore_ascii_case(&norm(b))
    }
    #[cfg(not(windows))]
    {
        norm(a) == norm(b)
    }
}
