export interface drbd_attr {
  name: string;
  provider: string;
  version: string;
  shortdesc: string;
  longdesc: string;
  parameters: Parameter[];
  actions: Action[];
}

export interface Parameter {
  name: string;
  unique: boolean;
  required: boolean;
  shortdesc: string;
  longdesc: string;
  type: string;
  default: string;
}

export interface Action {
  name: string;
  timeout: string;
  interval: string;
  depth: string;
}

export const drbd_attr_DATA: drbd_attr = {
  name: 'drbd-attr',
  provider: 'linbit',
  version: '1.0',
  shortdesc: 'import DRBD state change events as transient node attributes',
  longdesc:
    'This listens for DRBD state change events, and sets or deletes transient node\nattributes based on the "promotion_score" and "may_promote" values as presented\nby the DRBD events2 interface.\n\nOptionally using a dampening delay, see attrd_updater for details.\n\nTo be used as a clone on all DRBD nodes.  The idea is to start DRBD outside of\npacemaker, use DRBD auto-promote, and add location constraints for the\nFilesystem or other resource agents which are using DRBD.',
  parameters: [
    {
      name: 'dampening_delay',
      unique: false,
      required: false,
      shortdesc: 'attrd_updater --delay',
      longdesc: 'To be used as dampening delay in attrd_updater.',
      type: 'integer',
      default: '5',
    },
    {
      name: 'attr_name_prefix',
      unique: false,
      required: false,
      shortdesc: 'attrd_updater --name *prefix*-drbd_resource_name',
      longdesc:
        'The attributes will be named "*prefix*-drbd_resource_name".\nYou can chose that prefix here.',
      type: 'string',
      default: 'drbd-promotion-score',
    },
    {
      name: 'record_event_details',
      unique: false,
      required: false,
      shortdesc: '',
      longdesc:
        'It may be convenient to know which event lead to the current score.\nThis setting toggles the recording of the event.\nThe attributes will be named "*prefix*:event-details-drbd_resource_name".',
      type: 'boolean',
      default: 'false',
    },
  ],
  actions: [
    {
      name: 'start',
      timeout: '20s',
      interval: '',
      depth: '',
    },
    {
      name: 'stop',
      timeout: '20s',
      interval: '',
      depth: '',
    },
    {
      name: 'monitor',
      timeout: '20s',
      interval: '60s',
      depth: '0',
    },
    {
      name: 'validate-all',
      timeout: '20s',
      interval: '',
      depth: '',
    },
    {
      name: 'meta-data',
      timeout: '5s',
      interval: '',
      depth: '',
    },
  ],
};
