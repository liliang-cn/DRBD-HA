// DRBD HA Theme Colors
// 主品牌色
export const PRIMARY_COLOR = '#F79133';

// 主题色板
export const THEME_COLORS = [
  '#F79133', // 橙色 - 主色
  '#499BBB', // 蓝色
  '#E1C047', // 金棕色
  '#65BDED', // 天蓝色
  '#C0854E', // 铜棕色
  '#84E4E9', // 青色
  '#FF6D6D', // 粉红色
  '#5FD4A9', // 薄荷绿
  '#C38EC8', // 紫色
  '#BBD45F', // 橄榄绿
];

// 辅助色映射
export const ACCENT_COLORS = {
  orange: THEME_COLORS[0],
  blue: THEME_COLORS[1],
  gold: THEME_COLORS[2],
  sky: THEME_COLORS[3],
  copper: THEME_COLORS[4],
  cyan: THEME_COLORS[5],
  pink: THEME_COLORS[6],
  mint: THEME_COLORS[7],
  purple: THEME_COLORS[8],
  olive: THEME_COLORS[9],
};

// 根据索引获取渐变色
export function getColorGradient(index: number): [string, string] {
  const colors = THEME_COLORS;
  const color1 = colors[index % colors.length];
  const color2 = colors[(index + 1) % colors.length];
  return [color1, color2];
}

// 根据索引获取单一颜色
export function getColor(index: number): string {
  return THEME_COLORS[index % THEME_COLORS.length];
}
