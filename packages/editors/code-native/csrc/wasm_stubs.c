// wasm32 only: tree-sitter's alloc.c (in arborium-tree-sitter) calls
// fprintf(stderr, …) when an allocation fails. arborium's own stubs provide
// malloc/free/…, but `stderr` and `fprintf` come from arborium-sysroot, a crate
// nothing references and rustc therefore never links (P-114). Two symbols
// close the gap.
typedef struct { int unused; } FILE;
static FILE moonkale_stderr;
FILE *stderr = &moonkale_stderr;
int fprintf(FILE *stream, const char *format, ...) {
    (void)stream;
    (void)format;
    return 0;
}
