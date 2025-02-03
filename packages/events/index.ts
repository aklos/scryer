import { addEvent, getOrCreateVisitor } from "@repo/database";

interface UTMParams {
  campaign?: string;
  source?: string;
  medium?: string;
  term?: string;
  content?: string;
}

interface AdTracking {
  gclid?: string;
  fbclid?: string;
  msclkid?: string;
  twclid?: string;
  ttclid?: string;
  li_fat_id?: string;
}

interface ClickData {
  fieldLabel?: string;
}

interface FormData {
  formName: string;
  email?: string;
}

export interface EcommerceData {
  currency: string;
  value: number;
  items: {
    id: string;
    quantity: number;
  }[];
  shippingType?: string;
  paymentType?: string;
}

export type EventData = {
  event: string;
  origin: string;
  token: string;
  fingerprint: string;
  deviceType?: string;
  path?: string;
  pageTitle?: string;
  utmParams?: UTMParams;
  adTracking?: AdTracking;
  clickData?: ClickData;
  formData?: FormData;
  ecommerceData?: EcommerceData;
};

export async function handleEvent(payload: { data: EventData }) {
  const data = payload.data;
  const visitor = await getOrCreateVisitor(data.fingerprint);
  await addEvent(visitor.id, data);
}
