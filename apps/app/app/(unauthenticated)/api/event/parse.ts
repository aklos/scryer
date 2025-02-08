import { EventData } from "@repo/events";

export function parseEvent(body: any): [EventData, string | null] {
  const data = body as EventData;

  if (!data || typeof data !== "object") {
    return [data, "Event is not a valid object"];
  }

  // if (data.formData && typeof data.formData === "string") {
  //   data.formData = JSON.parse(data.formData);
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
      if (!data.clickData || !data.clickData.label) {
        return "Missing clickData field with 'label' property";
      }
      return null;
    case "link_click":
      if (!data.clickData || !data.clickData.href) {
        return "Missing clickData field with 'href' property";
      }
      return null;
    case "scroll_25_percent":
      return null;
    case "scroll_50_percent":
      return null;
    case "scroll_75_percent":
      return null;
    case "scroll_bottom":
      return null;
    default:
      return `Unrecognized event "${data.event}"`;
  }
}
