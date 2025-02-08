import {
  addEvent,
  getOrCreateVisitor,
  setVisitorHashedEmail,
  setVisitorLeadStatus,
} from "@repo/database";

interface UTMParams {
  campaign?: string;
  source?: string;
  medium?: string;
  term?: string;
  content?: string;
}

interface AdClickIds {
  google?: string;
  facebook?: string;
  microsoft?: string;
  twitter?: string;
  tiktok?: string;
  linkedin?: string;
}

interface ClickData {
  label?: string;
  href?: string;
}

interface FormData {
  formName: string;
  email?: string;
}

interface ProductData {
  name?: string;
  sku?: string;
  brand?: string;
  price?: string;
  currency?: string;
  availability?: string;
}

export type EventData = {
  event: string;
  origin: string;
  ip?: string;
  token: string;
  fingerprint?: string;
  newLead?: boolean;
  deviceType?: string;
  path?: string;
  pageTitle?: string;
  utmParams?: UTMParams;
  adClickIds?: AdClickIds;
  clickData?: ClickData;
  formData?: FormData;
  productData?: ProductData;
};

export async function handleEvent(payload: {
  accountId: string;
  data: EventData;
}) {
  const data = payload.data;

  if (!data.fingerprint) {
    return;
  }

  const visitor = await getOrCreateVisitor(
    payload.accountId,
    data.fingerprint,
    data.ip
  );

  if (!visitor) {
    return;
  }

  if (data.event !== "page_visit") {
    delete data.productData;
  }

  delete data.ip;
  delete data.fingerprint;

  if (data.formData?.email && visitor.lead_status === "non_lead") {
    data.newLead = true;
    await setVisitorLeadStatus(visitor.id, "lead");
  }

  if (data.formData?.email) {
    await setVisitorHashedEmail(visitor.id, data.formData.email);
  }

  if (data.event === "conversion") {
    await setVisitorLeadStatus(visitor.id, "converted");
  }

  await addEvent(visitor.id, data);
}
