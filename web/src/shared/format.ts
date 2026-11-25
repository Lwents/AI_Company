/** Number, money and time formatting (Vietnamese locale). */

/** Fixed exchange rate used for cost reports. */
export const USD_TO_VND = 25000;

export const vnd = (n: number) => `${Math.round(n).toLocaleString("vi-VN")} đ`;
export const usd = (n: number) => (n < 0.01 && n > 0 ? `$${n.toFixed(4)}` : `$${n.toFixed(2)}`);
export const num = (n: number) => n.toLocaleString("vi-VN");
export const time = (ts: number) => new Date(ts).toLocaleTimeString("vi-VN", { hour: "2-digit", minute: "2-digit", second: "2-digit" });
