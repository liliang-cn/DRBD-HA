// Service HA Wizard - For application services (MongoDB, MySQL, Redis, etc.)
import { Wizard } from "./Wizard";

export function ServiceHaWizard() {
  return <Wizard mode="service" />;
}
