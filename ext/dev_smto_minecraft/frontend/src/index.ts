import { faCubes } from '@fortawesome/free-solid-svg-icons';
import { Extension, ExtensionContext } from 'shared';
import PropertiesPage from './PropertiesPage.tsx';

class DevSmtoMinecraftExtension extends Extension {
  public cardConfigurationPage: React.FC | null = null;
  public cardComponent: React.FC | null = null;

  public initialize(ctx: ExtensionContext): void {
    ctx.extensionRegistry.enterRoutes((routes) =>
      routes.addServerRoute({
        name: 'server.properties',
        icon: faCubes,
        path: '/dev-smto-minecraft/properties',
        element: PropertiesPage,
        permission: 'minecraft-properties.read',
      }),
    );
  }
}

export default new DevSmtoMinecraftExtension();
