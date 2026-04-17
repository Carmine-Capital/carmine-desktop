import { type JSX } from 'solid-js';

import { Tab, TabList, TabPanel, Tabs } from '../components/Tabs';
import { GeneralSection } from './settings/GeneralSection';
import { MountsSection } from './settings/MountsSection';
import { OfflineSection } from './settings/OfflineSection';
import { AboutSection } from './settings/AboutSection';

export const Settings = (): JSX.Element => (
  <>
    <p class="section-heading">Paramètres</p>
    <Tabs defaultTab="general">
      <TabList>
        <Tab id="general">Général</Tab>
        <Tab id="mounts">Lecteurs</Tab>
        <Tab id="offline">Hors-ligne</Tab>
        <Tab id="about">À propos</Tab>
      </TabList>

      <TabPanel id="general">
        <GeneralSection />
      </TabPanel>
      <TabPanel id="mounts">
        <MountsSection />
      </TabPanel>
      <TabPanel id="offline">
        <OfflineSection />
      </TabPanel>
      <TabPanel id="about">
        <AboutSection />
      </TabPanel>
    </Tabs>
  </>
);
