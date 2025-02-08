export function getDeviceType() {
  const mobileRE = /mobile|iphone|ipod|blackberry|iemobile|opera mini/i;
  const tabletRE = /ipad|android(?!.*mobile)/i;

  if (tabletRE.test(navigator.userAgent)) return "tablet";
  if (mobileRE.test(navigator.userAgent)) return "mobile";
  return "desktop";
}

export const hasValues = (obj) =>
  Object.values(obj).some((value) => value !== undefined);
