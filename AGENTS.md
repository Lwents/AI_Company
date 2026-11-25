# AI Company

## Bối cảnh
- Mô phỏng công ty phần mềm do AI agent vận hành, giao diện văn phòng 2D. Ý tưởng gốc: `IDEA.md`. Hướng dẫn: `README.md`.
- Backend Rust (`backend/`), frontend React + Vite + Canvas (`web/`).
- Trả lời bằng tiếng Việt, ngắn gọn.

## Lệnh
- Build + chạy: `powershell -ExecutionPolicy Bypass -File .\build.ps1 -Run` (toolchain GNU + MinGW; `cargo` với MSVC sẽ lỗi `link.exe not found`).
- Dev giao diện: chạy backend, rồi `npm run dev` (cổng 5173, proxy `/api`, `/ws`, `/preview` sang 8787).
- Test: `cd backend; cargo test` và `npm run typecheck`.

## Quy ước
- Không commit `data/`, `workspace/`, `.env`, `backend/target/`, `web/dist/`.
- Không ghi API key vào code, log hay tài liệu.
- Server chỉ bind `127.0.0.1`: agent chạy được lệnh shell.
- Gọi Claude bằng raw HTTP (Rust không có SDK chính thức): header `anthropic-version: 2026-10-21`, `x-api-key`.
- Kiểu dữ liệu ở `backend/src/types.rs` và `web/src/types.ts` phải khớp nhau (JSON camelCase).
