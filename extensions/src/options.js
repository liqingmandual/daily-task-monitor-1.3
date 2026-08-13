const optionsApi = globalThis.browser ?? globalThis.chrome;
const endpoint = document.querySelector("#endpoint");
const token = document.querySelector("#token");
const status = document.querySelector("#status");

void optionsApi.storage.local.get(["endpoint", "token"]).then((saved) => {
  if (saved.endpoint) endpoint.value = saved.endpoint;
  if (saved.token) token.value = saved.token;
});
document.querySelector("#save").addEventListener("click", async () => {
  await optionsApi.storage.local.set({ endpoint: endpoint.value.trim(), token: token.value.trim() });
  status.textContent = "已保存；请保持桌面应用运行。";
});
