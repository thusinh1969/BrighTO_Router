# FIX ROUND 2 — chi sua 2 loi clippy, KHONG doi thiet ke

File: `src/proxy/mod.rs`. Hai loi clippy -D warnings con sot (khong auto-fix duoc):

```
error: this function has too many arguments (8/7)  --> src/proxy/mod.rs:299 (fn finish)
error: this function has too many arguments (8/7)  --> src/proxy/mod.rs:325 (fn build_event)
```

Cach xu ly (chon 1 trong 2, ghi ro chon nao + TAI SAO trong comment):
- Gom cac tham so phoi canh vao 1 struct nho (vi du `struct FinishCtx`), GIU NGUYEN chu ky ben ngoai cua ham public.
- HOAC neu day la ham private noi bo va 8 la hop ly, them `#[allow(clippy::too_many_arguments)]` + comment giai thich.
KHONG doi logic. KHONG doi file khac. Tra ve DUNG file da sua theo format `// FILE: src/proxy/mod.rs` + code block.
