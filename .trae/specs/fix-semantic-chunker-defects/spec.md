# Fix SemanticChunker Defects Spec

## Why
Dokumen "VAC Implementation Plan" yang diberikan telah dievaluasi. Sebagian besar temuan (F-01 hingga F-07) ternyata **sudah diperbaiki atau tidak valid** (seperti `ChangesetStore` yang sudah menggunakan `HashMap` untuk O(1) lookup, dan `wait_for_record` untuk polling approval). 

Namun, temuan **F-08** (`SemanticChunker::chunk` Off-By-One Byte Count + Break Guard Terlalu Ketat) terbukti **akurat** dan masih ada di codebase. Cacat ini menyebabkan off-by-one byte counting pada alokasi memori chunk dan yang lebih parah, dapat memicu *infinite loop* ketika ukuran window mendekati atau sama dengan batas `chunk_overlap`. Hal ini dapat menyebabkan macet (hang) pada proses *indexing codebase*.

## What Changes
- Memperbaiki off-by-one byte count pada saat memotong (chunking) kata di `crates/vil_context/src/chunking.rs`.
- Menulis ulang implementasi sliding window `SemanticChunker::chunk` dengan memastikan *step* increment maju minimal 1 (`max(1)`), sehingga *infinite loop* dapat dihindari.
- Menambahkan test yang membuktikan *infinite loop* berhasil diatasi, serta memvalidasi algoritma windowing.
- Menambahkan validasi konstruktor `SemanticChunker::new` untuk memastikan parameter aman (misal `chunk_size > 0` dan `chunk_overlap < chunk_size`).

## Impact
- Affected specs: Text Chunking and Indexing.
- Affected code: `crates/vil_context/src/chunking.rs`

## MODIFIED Requirements
### Requirement: Aman dari Infinite Loop pada Edge Case Chunk Overlap
Sistem SHALL menyediakan pemotongan chunk yang beroperasi tanpa ada risiko infinite loop, dan menggunakan memori yang presisi dalam penghitungan byte, dengan overlapping yang konsisten.

#### Scenario: Success case
- **WHEN** user memasukkan teks yang secara spesifik memiliki total kata berjumlah `chunk_overlap + 1` hingga batas rentan,
- **THEN** fungsi `chunk` tetap mengembalikan *array of strings* secara berurutan dan tidak menggantung (infinite loop).