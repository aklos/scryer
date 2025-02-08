import { WebServiceClient } from "@maxmind/geoip2-node";
import { keys } from "./keys";

const client = new WebServiceClient(
  keys().GEOIP_ACCOUNT_ID || "",
  keys().GEOIP_LICENSE_KEY || "",
  { host: "geolite.info" }
);

export function getCountryFromIP(ip: string) {
  return client.country(ip);
}
