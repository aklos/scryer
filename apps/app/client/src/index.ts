import * as ThumbmarkJS from "@thumbmarkjs/thumbmarkjs";
import { getDeviceType, setupEventListeners } from "./utils";

declare global {
  interface Window {
    // scryer: TrackingClientClass;
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

interface EcommerceData {
  currency: string;
  value: number;
  items: {
    id: string;
    quantity: number;
  }[];
  shippingType?: string;
  paymentType?: string;
}

type EventData = {
  event: string;
  fingerprint?: string;
  deviceType?: string;
  path?: string;
  pageTitle?: string;
  utmParams?: UTMParams;
  adTracking?: AdTracking;
  clickData?: ClickData;
  formData?: FormData;
  ecommerceData?: EcommerceData;
};

interface ScryerClientClass {
  init: () => void;
  sendEvent: (data: EventData) => void;
  // conversion: () => void;
}

class ScryerClient implements ScryerClientClass {
  private apiUrl: string;
  private token: string;

  private setupEventListeners = setupEventListeners.bind(this);

  private eventQueue: { eventData: EventData; asBeacon: boolean }[] = [];

  private fingerprint: string = "";
  private deviceType: string = "";

  /**
   * Constructor for the ScryerClient class
   */
  constructor() {
    this.apiUrl = "{{ API_URL }}";
    this.token = "{{ TOKEN }}";

    // Persist if lifecycle destroys the instance
    // window.scryer = this;
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

    this.setupEventListeners();

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

    const adTracking = {
      gclid: params.gclid || undefined,
      fbclid: params.fbclid || undefined,
      msclkid: params.msclkid || undefined,
      twclid: params.twclid || undefined,
      ttclid: params.ttclid || undefined,
      li_fat_id: params.li_fat_id || undefined,
    };

    const body = {
      token: this.token,
      event: eventData.event,
      fingerprint: this.fingerprint,
      deviceType: this.deviceType,
      path: window.location.pathname,
      pageTitle: document.title,
      utmParams,
      adTracking,
      formData: eventData.formData,
      ecommerceData: eventData.ecommerceData,
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
            console.log(`Scryer: Sent ${body.event} event`);
            return res.text();
          } else {
            throw new Error(res.statusText);
          }
        })
        .catch((err) => {
          console.log(err);
        });
    } else {
      console.log("Scryer: Sending beacon");
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

  /**
   * addToCart
   *
   * @description Tracks when an item is added to the cart
   *
   * @param {EcommerceData} ecommerceData - Potential product/purchase details.
   */
  public addToCart(ecommerceData: EcommerceData) {
    this.sendEvent(
      {
        event: "add_to_cart",
        ecommerceData: Object.assign(
          { currency: "USD", value: 0, items: [] },
          ecommerceData
        ),
      },
      true
    );
  }

  /**
   * removeFromCart
   *
   * @description Tracks when an item is removed from the cart
   *
   * @param {EcommerceData} ecommerceData - Potential product/purchase details.
   */
  public removeFromCart(ecommerceData: EcommerceData) {
    this.sendEvent(
      {
        event: "remove_from_cart",
        ecommerceData: Object.assign(
          { currency: "USD", value: 0, items: [] },
          ecommerceData
        ),
      },
      true
    );
  }

  /**
   * purchase
   *
   * @description Tracks when a purchase is made
   *
   * @param {EcommerceData} ecommerceData - Potential product/purchase details.
   */
  public purchase(ecommerceData: EcommerceData) {
    this.sendEvent(
      {
        event: "purchase",
        ecommerceData: Object.assign(
          { currency: "USD", value: 0, items: [] },
          ecommerceData
        ),
      },
      true
    );
  }

  /**
   * refund
   *
   * @description Tracks when a refund is issued
   *
   * @param {EcommerceData} ecommerceData - Potential product/purchase details.
   */
  public refund(ecommerceData: EcommerceData) {
    this.sendEvent(
      {
        event: "refund",
        ecommerceData: Object.assign(
          { currency: "USD", value: 0, items: [] },
          ecommerceData
        ),
      },
      true
    );
  }

  /**
   * addPaymentInfo
   *
   * @description Tracks when payment information is added
   *
   * @param {EcommerceData} ecommerceData - Potential product/purchase details.
   */
  public addPaymentInfo(ecommerceData: EcommerceData) {
    this.sendEvent(
      {
        event: "add_payment_info",
        ecommerceData: Object.assign(
          { currency: "USD", value: 0, items: [] },
          ecommerceData
        ),
      },
      true
    );
  }

  /**
   * addShippingInfo
   *
   * @description Tracks when shipping information is added
   *
   * @param {EcommerceData} ecommerceData - Potential product/purchase details.
   */
  public addShippingInfo(ecommerceData: EcommerceData) {
    this.sendEvent(
      {
        event: "add_shipping_info",
        ecommerceData: Object.assign(
          { currency: "USD", value: 0, items: [] },
          ecommerceData
        ),
      },
      true
    );
  }

  /**
   * beginCheckout
   *
   * @description Tracks when the checkout process begins
   *
   * @param {EcommerceData} ecommerceData - Potential product/purchase details.
   */
  public beginCheckout(ecommerceData: EcommerceData) {
    this.sendEvent(
      {
        event: "begin_checkout",
        ecommerceData: Object.assign(
          { currency: "USD", value: 0, items: [] },
          ecommerceData
        ),
      },
      true
    );
  }

  /**
   * viewItem
   *
   * @description Tracks when a specific item is viewed
   *
   * @param {EcommerceData} ecommerceData - Potential product/purchase details.
   */
  public viewItem(ecommerceData: EcommerceData) {
    this.sendEvent(
      {
        event: "view_item",
        ecommerceData: Object.assign(
          { currency: "USD", value: 0, items: [] },
          ecommerceData
        ),
      },
      true
    );
  }

  /**
   * viewItemList
   *
   * @description Tracks when a specific item is viewed
   *
   * @param {EcommerceData} ecommerceData - Potential product/purchase details.
   */
  public viewItemList(ecommerceData: EcommerceData) {
    this.sendEvent(
      {
        event: "view_item_list",
        ecommerceData: Object.assign(
          { currency: "USD", value: 0, items: [] },
          ecommerceData
        ),
      },
      true
    );
  }

  /**
   * viewCart
   *
   * @description Tracks when the cart is viewed
   *
   * @param {EcommerceData} ecommerceData - Potential product/purchase details.
   */
  public viewCart(ecommerceData: EcommerceData) {
    this.sendEvent(
      {
        event: "view_cart",
        ecommerceData: Object.assign(
          { currency: "USD", value: 0, items: [] },
          ecommerceData
        ),
      },
      true
    );
  }

  /**
   * ecommerceEvent
   *
   * @description Generic ecommerce event trigger
   *
   * @param {{ event: string, ecommerceData: EcommerceData }} data - Event name and potential product/purchase details.
   */
  public ecommerceEvent(data: { event: string; ecommerceData: EcommerceData }) {
    this.sendEvent(
      {
        event: data.event,
        ecommerceData: Object.assign(
          { currency: "USD", value: 0, items: [] },
          data.ecommerceData
        ),
      },
      true
    );
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
