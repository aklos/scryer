import * as ThumbmarkJS from "@thumbmarkjs/thumbmarkjs";
import Autocapture from "./autocapture";
import { getDeviceType, hasValues } from "./utils";

declare global {
  interface Window {
    jQuery?: any;
    __SCRYER_INITIALIZED__?: boolean;
  }
}

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

type EventData = {
  event: string;
  fingerprint?: string;
  deviceType?: string;
  path?: string;
  pageTitle?: string;
  utmParams?: UTMParams;
  adClickIds?: AdClickIds;
  clickData?: ClickData;
  formData?: FormData;
  productData?: ProductData;
};

interface ScryerClientClass {
  init: () => void;
  sendEvent: (data: EventData, asBeacon: boolean) => void;
  conversion: () => void;
}

class ScryerClient implements ScryerClientClass {
  private apiUrl: string;
  private token: string;

  private autocapture: Autocapture;

  private eventQueue: { eventData: EventData; asBeacon: boolean }[] = [];

  private deviceType: string = "";
  public fingerprint: string = "";

  /**
   * Constructor for the ScryerClient class
   */
  constructor() {
    this.apiUrl = "{{ API_URL }}";
    this.token = "{{ TOKEN }}";
    this.autocapture = new Autocapture(this);

    // Persist if lifecycle destroys the instance
    // window.scryer = this;
  }

  private extractProductSchema() {
    const scripts = Array.from(
      document.querySelectorAll('script[type="application/ld+json"]')
    );

    for (const script of scripts) {
      try {
        const jsonData = JSON.parse((script as any).innerText);

        // Check if it's a Product schema
        if (jsonData["@type"] === "Product") {
          return {
            name: jsonData.name || undefined,
            sku: jsonData.sku || undefined,
            brand: jsonData.brand?.name || undefined,
            price: jsonData.offers?.price || undefined,
            lowPrice: jsonData.offers?.lowPrice || undefined,
            highPrice: jsonData.offers?.highPrice || undefined,
            currency: jsonData.offers?.priceCurrency || undefined,
            availability: jsonData.offers?.availability || undefined,
          };
        }
      } catch (error) {
        console.warn("Error parsing ld+json:", error);
      }
    }

    return {};
  }

  /**
   * init
   *
   * This method initializes the tracking and sets up the necessary callbacks.
   * It retrieves the visitor's fingerprint.
   *
   * @returns {Promise} A Promise that resolves when the tracking is initialized.
   */
  public async init() {
    this.fingerprint = await ThumbmarkJS.getFingerprint();
    this.deviceType = getDeviceType();

    this.sendEvent({
      event: "page_visit",
    });

    this.autocapture.startTracking();

    if (this.eventQueue.length > 0) {
      for (const e of this.eventQueue) {
        this.sendEvent(e.eventData, e.asBeacon);
      }
    }
  }

  public sendEvent(eventData: EventData, asBeacon = false) {
    if (!this.fingerprint) {
      this.eventQueue.push({ eventData, asBeacon });
      return;
    }

    const params: any = new Proxy(new URLSearchParams(window.location.search), {
      get: (searchParams, prop: string) => searchParams.get(prop),
    });

    const utmParams = {
      campaign: params.utm_campaign || undefined,
      source: params.utm_source || undefined,
      medium: params.utm_medium || undefined,
      term: params.utm_term || undefined,
      content: params.utm_content || undefined,
    };

    const adClickIds = {
      google: params.gclid || undefined,
      facebook: params.fbclid || undefined,
      microsoft: params.msclkid || undefined,
      twitter: params.twclid || undefined,
      tiktok: params.ttclid || undefined,
      linkedin: params.li_fat_id || undefined,
    };

    const productData = this.extractProductSchema();

    const body = {
      token: this.token,
      event: eventData.event,
      fingerprint: this.fingerprint,
      deviceType: this.deviceType,
      path: window.location.pathname,
      pageTitle: document.title,
      utmParams: hasValues(utmParams) ? utmParams : undefined,
      adClickIds: hasValues(adClickIds) ? adClickIds : undefined,
      formData: eventData.formData,
      clickData: eventData.clickData,
      productData: hasValues(productData) ? productData : undefined,
    };

    if (!asBeacon) {
      const controller = new AbortController();
      const timeoutId = setTimeout(() => controller.abort(), 3000);
      fetch(this.apiUrl + "/api/event", {
        method: "POST",
        body: JSON.stringify(Object.assign({}, body)),
        headers: {
          "Content-Type": "application/json",
        },
        credentials: "include",
        signal: controller.signal,
        keepalive: true,
      })
        .then((res) => {
          clearTimeout(timeoutId);

          if (res.ok) {
            console.log(`Scryer: Sent ${body.event} event.`);
            return res.text();
          } else {
            throw new Error(res.statusText);
          }
        })
        .catch((err) => {
          console.log(err);
        });
    } else {
      console.log(`Scryer: Sending beacon for ${body.event} event.`);
      const jsonData = JSON.stringify(body);
      const blob = new Blob([jsonData], { type: "application/json" });
      navigator.sendBeacon(this.apiUrl + "/api/event", blob);
    }
  }

  /**
   * conversion
   *
   * @description Generates conversion event for visitor
   */
  public conversion() {
    this.sendEvent({
      event: "conversion",
    });
  }
}

const client = new ScryerClient();

function onReady() {
  client.init();
  window.__SCRYER_INITIALIZED__ = true; // Mark initialized
  console.log("Scryer client initialized.");
}

// Auto-init for script users (Waits for full page load)
queueMicrotask(() => {
  if (!document.currentScript || document.currentScript.type !== "module") {
    if (!window.__SCRYER_INITIALIZED__) {
      if (document.readyState === "complete") {
        onReady();
      } else {
        window.addEventListener("load", onReady);
      }
    }
  }
});

// Default Export (for script-based usage)
export { client, ScryerClient };
