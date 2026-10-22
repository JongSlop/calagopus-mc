import { faCubes, faPuzzlePiece, faUsers, faWrench } from '@fortawesome/free-solid-svg-icons';
import { Extension, ExtensionContext } from 'shared';
import MinecraftSettingsPage from './MinecraftSettingsPage.tsx';
import ModsPage from './ModsPage.tsx';
import PropertiesPage from './PropertiesPage.tsx';
import WhitelistPage from './WhitelistPage.tsx';

class DevSmtoMinecraftExtension extends Extension {
  public cardConfigurationPage: React.FC | null = null;
  public cardComponent: React.FC | null = null;

  public initialize(ctx: ExtensionContext): void {
    ctx.extensionRegistry.enterRoutes((routes) => {
      routes.addServerRoute({
        name: 'server.properties',
        icon: faCubes,
        path: '/dev-smto-minecraft/properties',
        element: PropertiesPage,
        permission: 'minecraft-properties.read',
      });

      routes.addServerRoute({
        name: 'Whitelist',
        icon: faUsers,
        path: '/dev-smto-minecraft/whitelist',
        element: WhitelistPage,
        permission: 'minecraft-whitelist.read',
      });

      routes.addServerRoute({
        name: 'Mods',
        icon: faPuzzlePiece,
        path: '/dev-smto-minecraft/mods',
        element: ModsPage,
        permission: 'minecraft-mods.read',
      });

      routes.addAdminRoute({
        name: 'Minecraft Tools',
        icon: faWrench,
        path: '/extensions/dev-smto-minecraft',
        element: MinecraftSettingsPage,
        permission: 'extensions.read',
        category: 'extensions',
      });
    });
  }
}

export default new DevSmtoMinecraftExtension();
