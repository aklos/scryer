import { EcommerceData, EventData } from "@repo/events";

export function parseEvent(body: any): [EventData, string | null] {
  const data = body as EventData;

  if (!data || typeof data !== "object") {
    return [data, "Event is not a valid object"];
  }

  // if (data.formData && typeof data.formData === "string") {
  //   data.formData = JSON.parse(data.formData);
  // }

  // if (data.ecommerceData && typeof data.ecommerceData === "string") {
  //   data.ecommerceData = JSON.parse(data.ecommerceData);

  //   if (typeof data.ecommerceData.value === "string") {
  //     data.ecommerceData.value = parseFloat(data.ecommerceData.value);
  //   }

  //   if (data.ecommerceData.items && data.ecommerceData.items.length) {
  //     data.ecommerceData.items.forEach((item, index) => {
  //       if (typeof item.quantity === "string") {
  //         data.ecommerceData.items[index].quantity = parseInt(item.quantity);
  //       }
  //     });
  //   }
  // }

  const validationError = validate(data);

  return [data, validationError];
}

function validate(data: EventData) {
  if (!data.fingerprint) {
    return "Missing fingerprint";
  }

  switch (data.event) {
    case "page_visit":
      return null;
    case "form_submission":
      if (!data.formData) {
        return "Missing formData field";
      }

      return null;
    case "conversion":
      return null;
    case "button_click":
      if (!data.clickData || !data.clickData.fieldLabel) {
        return "Missing clickData field with 'fieldLabel' property";
      }

      return null;
    case "add_to_cart":
      return validateEcommerceData(data.event, data.ecommerceData);
    case "remove_from_cart":
      return validateEcommerceData(data.event, data.ecommerceData);
    case "purchase":
      return validateEcommerceData(data.event, data.ecommerceData);
    case "refund":
      return validateEcommerceData(data.event, data.ecommerceData);
    case "add_payment_info":
      return validateEcommerceData(data.event, data.ecommerceData);
    case "add_shipping_info":
      return validateEcommerceData(data.event, data.ecommerceData);
    case "begin_checkout":
      return validateEcommerceData(data.event, data.ecommerceData);
    case "view_item":
      return validateEcommerceData(data.event, data.ecommerceData);
    case "view_item_list":
      return validateEcommerceData(data.event, data.ecommerceData);
    case "view_cart":
      return validateEcommerceData(data.event, data.ecommerceData);
    default:
      return `Unrecognized event "${data.event}"`;
  }
}

function validateEcommerceData(event: string, data: EcommerceData | undefined) {
  if (!data) {
    return "Missing ecommerceData";
  }

  if (!data.currency) return "Missing ecommerce 'currency' property";
  if (data.value !== 0 && !data.value)
    return "Missing ecommerce 'value' property";

  const basicEvents = [
    "add_to_cart",
    "remove_from_cart",
    "purchase",
    "refund",
    "begin_checkout",
    "view_item",
    "view_item_list",
  ];

  if (basicEvents.includes(event)) {
    if (!data.items || data.items.length === 0)
      return "Missing ecommerce 'items' property or entries";
  } else if (event === "add_payment_info" && !data.paymentType) {
    return "Missing ecommerce 'paymentType' property";
  } else if (event === "add_shipping_info" && !data.shippingType) {
    return "Missing ecommerce 'shippingType' property";
  }

  if (data.items && data.items.length) {
    const passed = data.items.reduce((accu, curr) => {
      if (!curr.id) return false;
      if (
        curr.quantity === null ||
        curr.quantity === undefined ||
        isNaN(curr.quantity)
      )
        return false;

      return accu;
    }, true);

    if (!passed) {
      return "Some ecommerce items missing 'id' or 'quantity' properties";
    }
  }

  return null;
}
