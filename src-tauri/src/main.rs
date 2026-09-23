// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(all(windows, feature = "silero-te"))]
    {
        // libtorch often ships mkl_core without AVX512 dispatch DLLs; avoid fatal load on dev machines.
        std::env::set_var("MKL_DEBUG_CPU_TYPE", "5");
        std::env::set_var("KMP_DUPLICATE_LIB_OK", "TRUE");
    }
    veyro_lib::run()
}
