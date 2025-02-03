import slugify from "slugify";
import type { ScryerClient } from "./index";

export function getDeviceType() {
  const mobileRE =
    /(android|bb\d+|meego).+mobile|armv7l|avantgo|bada\/|blackberry|blazer|compal|elaine|fennec|hiptop|iemobile|ip(hone|od)|iris|kindle|lge |maemo|midp|mmp|mobile.+firefox|netfront|opera m(ob|in)i|palm( os)?|phone|p(ixi|re)\/|plucker|pocket|psp|series[46]0|samsungbrowser.*mobile|symbian|treo|up\.(browser|link)|vodafone|wap|windows (ce|phone)|xda|xiino/i;
  const notMobileRE = /CrOS/;
  const tabletRE = /android|ipad|playbook|silk/i;

  const mobile =
    mobileRE.test(navigator.userAgent) &&
    !notMobileRE.test(navigator.userAgent);
  const tablet = !mobile && tabletRE.test(navigator.userAgent);

  if (tablet) {
    return "tablet";
  }

  if (mobile) {
    return "mobile";
  }

  return "desktop";
}

export function setupEventListeners(this: ScryerClient) {
  const regexPatterns = [
    {
      pattern: /email/i,
      fieldName: "email",
    },
  ];

  const findFirstMatch = (inputs) => {
    for (const entry of regexPatterns) {
      for (const input of inputs) {
        if (entry.pattern.test(input)) {
          return entry.fieldName;
        }
      }
    }

    return null;
  };

  const findForm = (elem) => {
    while (elem) {
      if (elem.nodeName === "FORM") {
        return elem as HTMLFormElement;
      }

      elem = elem.parentNode;
    }

    return null;
  };

  const extractFormIdentifier = (form) => {
    if (form.getAttribute("name")) return form.getAttribute("name");
    if (form.getAttribute("id")) return form.getAttribute("id");
    if (form.getAttribute("data-form-name"))
      return form.getAttribute("data-form-name");

    try {
      if (form.attributes && typeof form.attributes === "object") {
        const livewireModel = Object.values(form.attributes).find(
          (attr: any) => attr.name && attr.name.startsWith("wire:model")
        );
        if (livewireModel && (livewireModel as any).value) {
          return (livewireModel as any).value;
        }
      }
    } catch (error) {
      console.warn("Error extracting form identifier:", error);
    }

    return (
      form.getAttribute("action") ||
      window.location.pathname + window.location.search
    );
  };

  const handledForms: HTMLFormElement[] = [];

  const baseHandler = (form: HTMLFormElement) => {
    const formData = new FormData(form);
    const data: any = {};

    formData.forEach((value, key) => {
      const fields = document.getElementsByName(key);
      if (fields.length > 0) {
        const field = fields[0];

        const id = field.getAttribute("id");
        const type = field.getAttribute("type");
        const placeholder = field.getAttribute("placeholder");

        let labelElem = document.querySelector(`label[for="${key}"]`);

        if (!labelElem) {
          if (field.parentNode?.nodeName === "LABEL") {
            labelElem = field.parentNode as Element;
          } else if (
            field.previousElementSibling &&
            field.previousElementSibling.nodeName === "LABEL"
          ) {
            labelElem = field.previousElementSibling;
          } else if (
            field.nextElementSibling &&
            field.nextElementSibling.nodeName === "LABEL"
          ) {
            labelElem = field.nextElementSibling;
          }
        }

        const label = labelElem ? labelElem.textContent : "";

        const checks = [type, placeholder, label, id];

        const fieldName =
          findFirstMatch(checks) || slugify(key, { strict: true, lower: true });

        data[fieldName] = value;
      }
    });

    data.formName = extractFormIdentifier(form);

    this.sendEvent(
      {
        event: "form_submission",
        formData: data,
      },
      true
    );
  };

  const buttonHandler = (e: Event) => {
    const button = e.target as HTMLButtonElement | HTMLInputElement;

    if (button.disabled) {
      return;
    }

    const formElem = findForm(button);
    if (!formElem) {
      return;
    } else {
      const jQuery = window.jQuery;
      const isFormValid =
        typeof jQuery !== "undefined" && typeof jQuery.fn.valid === "function"
          ? jQuery(formElem).valid()
          : formElem.checkValidity();
      if (!isFormValid) {
        return;
      }
    }

    if (handledForms.includes(formElem)) {
      return;
    }

    // Wait for formHandler
    setTimeout(() => {
      if (!handledForms.includes(formElem)) {
        baseHandler(formElem);
      }
    }, 100);
  };

  const formHandler = (e: SubmitEvent) => {
    const formElem = e.target as HTMLFormElement;

    if (handledForms.includes(formElem)) {
      return;
    }

    handledForms.push(formElem);
    setTimeout(() => {
      const formIndex = handledForms.findIndex((f) => f === formElem);
      handledForms.splice(formIndex, 1);
    }, 1000);

    const jQuery = window.jQuery;
    const isFormValid =
      typeof jQuery !== "undefined" && typeof jQuery.fn.valid === "function"
        ? jQuery(formElem).valid()
        : formElem.checkValidity();

    if (isFormValid) {
      baseHandler(formElem);
    }
  };

  const clickHandler = (e: Event) => {
    const button = e.target as HTMLButtonElement | HTMLInputElement;

    if (button.disabled) {
      return;
    }

    const label = button.innerText || button.getAttribute("name");

    this.sendEvent({
      event: "button_click",
      clickData: { fieldLabel: label || undefined },
    });
  };

  const addEventListeners = () => {
    const buttons = document.querySelectorAll("button, input[type='submit']");

    for (let i = 0; i < buttons.length; i++) {
      const btn = buttons[i] as HTMLButtonElement | HTMLInputElement;
      if (btn.type === "submit") {
        btn.removeEventListener("click", buttonHandler);
        btn.addEventListener("click", buttonHandler);
      } else {
        btn.removeEventListener("click", clickHandler);
        btn.addEventListener("click", clickHandler);
      }
    }

    const forms = document.getElementsByTagName("form");

    for (let i = 0; i < forms.length; i++) {
      const form = forms[i] as HTMLFormElement;
      form.removeEventListener("submit", formHandler);
      form.addEventListener("submit", formHandler);
    }
  };

  const observer = new MutationObserver((mutations) => {
    addEventListeners();
  });

  observer.observe(document, { childList: true, subtree: true });
  addEventListeners();
}
