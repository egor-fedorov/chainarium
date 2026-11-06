export const MAX = (1n << 256n) - 1n;
export function atoms(value) {
  if (typeof value !== 'string' || !/^(0|[1-9][0-9]*)$/.test(value) || value.length > 78 || BigInt(value) > MAX) throw Error('Ожидается целое число от 0 до U256::MAX');
  return value;
}
export function units(text, decimals) {
  if (!Number.isInteger(decimals) || decimals < 0 || decimals > 255 || !/^(0|[1-9][0-9]*)(\.[0-9]+)?$/.test(text)) throw Error('Некорректная сумма');
  const [whole, fraction = ''] = text.split('.');
  if (fraction.length > decimals) throw Error('Слишком много знаков после запятой');
  return atoms((whole + fraction + '0'.repeat(decimals - fraction.length)).replace(/^0+(?=\d)/, ''));
}
export function formatUnits(value, decimals) {
  atoms(value);
  const s = value.padStart(decimals + 1, '0');
  return decimals ? s.slice(0, -decimals) + '.' + s.slice(-decimals) : s;
}
