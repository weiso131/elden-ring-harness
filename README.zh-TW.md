# elden-ring-harness

法環的模組，把遊戲內部接出來給實驗腳本用：記錄主執行緒每一幀把時間花在哪、傳送到固定的
起點、重置 boss、重播錄好的路線，讓掉幀能重現，並從遊戲內部找出原因。

狀態：研究階段，還沒有程式碼。見 [docs/research.md](docs/research.md)。

## 只在實驗時使用

平常玩不會載入這個模組。從 Steam 啟動會走 `start_protected_game.exe`，EAC 開著，遊戲維持原狀。
做實驗時改用 me3 模組載入器和專用的 profile 啟動，離線、使用另一個存檔。
